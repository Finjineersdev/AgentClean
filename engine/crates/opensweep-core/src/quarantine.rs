//! Quarantine: cleaning moves, it does not delete.
//!
//! A clean is a *run*. Each run gets a folder in the store:
//!
//! ```text
//! <store>/runs/<run id>/manifest.json
//! <store>/runs/<run id>/items/0000, 0001, ...
//! ```
//!
//! The manifest is written before the first move (invariant I14) and
//! rewritten after each one, so a crash leaves at worst an entry marked
//! `pending` whose item is already in `items/`; restore and purge treat such
//! an entry as moved. Items are moved by rename on the same volume
//! (invariant I13), so a clean never copies data and never unlinks it.
//! Space comes back only when a run is purged.
//!
//! A folder on another local drive goes to a quarantine on that drive,
//! `<drive>/.opensweep-quarantine/runs/<run id>/items/` (ADR 0004). The
//! manifest stays in the main store and records that folder as the entry's
//! `volume`; every folder on the way must be a real folder that only its
//! owner, the owner of the main store, can change (invariant I19).

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::guard::{rename_into, Approved, MoveFailed, Refusal};
use crate::rules::Class;
use crate::size;
use crate::volumes::{self, Mount};

pub const MANIFEST: &str = "manifest.json";
pub const FORMAT: u32 = 1;
/// Name of the quarantine folder at the top of another drive.
pub const VOLUME_STORE: &str = ".opensweep-quarantine";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    /// Recorded, not yet moved.
    Pending,
    /// In quarantine.
    Moved,
    /// Not moved: it changed since approval or is on another volume.
    Skipped,
    /// The move was attempted and failed; the original is untouched.
    Failed,
    /// Moved back to its original location.
    Restored,
    /// Permanently deleted from quarantine.
    Purged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub original: PathBuf,
    pub project_root: PathBuf,
    /// File name under `items/`: four or more ASCII digits.
    pub stored: String,
    pub rule_id: String,
    pub class: Class,
    /// Allocated bytes at clean time.
    pub bytes: u64,
    pub state: State,
    /// The drive quarantine holding the item, when it is not the main store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<PathBuf>,
    /// Why the last action on this entry did not succeed, if it did not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    pub run_id: String,
    pub created_unix: u64,
    pub entries: Vec<Entry>,
}

impl Manifest {
    /// Bytes still held in quarantine by this run.
    pub fn bytes_held(&self) -> u64 {
        self.entries
            .iter()
            .filter(|e| e.state == State::Moved)
            .map(|e| e.bytes)
            .sum()
    }

    /// Bytes this run removed from the projects, whatever happened since.
    pub fn bytes_cleaned(&self) -> u64 {
        self.entries
            .iter()
            .filter(|e| matches!(e.state, State::Moved | State::Purged))
            .map(|e| e.bytes)
            .sum()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("quarantine folder {0}: {1}")]
    Store(PathBuf, String),
    #[error("nothing to clean")]
    EmptyPlan,
    #[error("no run named {0:?}")]
    UnknownRun(String),
    #[error("run {0}: manifest cannot be read: {1}")]
    BadManifest(String, String),
    #[error("run {0} has no item {1}")]
    UnknownItem(String, usize),
    #[error("run {0}: {1}")]
    Io(String, String),
}

/// Where the quarantine lives by default.
///
/// macOS: `~/Library/Application Support/OpenSweep/Quarantine`.
/// Elsewhere: `$XDG_DATA_HOME/opensweep/quarantine`, or
/// `~/.local/share/opensweep/quarantine`.
pub fn default_location() -> Option<PathBuf> {
    let loc = location_for(
        cfg!(target_os = "macos"),
        std::env::var_os("HOME").map(PathBuf::from),
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from),
    );
    if let Some(dir) = loc.as_deref().and_then(Path::parent) {
        adopt_legacy(dir);
    }
    loc
}

/// OpenSweep was called AgentClean until November 2026. When `new` (an
/// `OpenSweep` or `opensweep` folder) does not exist yet and the old folder
/// beside it does, the old one is renamed to it whole: never copied, never
/// merged, never when `new` exists, and never through a symbolic link.
/// Manifests store items relative to the store, so restore keeps working.
pub fn adopt_legacy(new: &Path) {
    let old_name = match new.file_name().and_then(|n| n.to_str()) {
        Some("OpenSweep") => "AgentClean",
        Some("opensweep") => "agentclean",
        _ => return,
    };
    let old = new.with_file_name(old_name);
    if fs::symlink_metadata(new).is_err() && fs::symlink_metadata(&old).is_ok_and(|m| m.is_dir()) {
        // ponytail: a failed rename leaves the old folder in place; the
        // engine then starts an empty store and the old runs stay on disk.
        let _ = fs::rename(&old, new);
    }
}

fn location_for(macos: bool, home: Option<PathBuf>, xdg: Option<PathBuf>) -> Option<PathBuf> {
    if macos {
        return home.map(|h| h.join("Library/Application Support/OpenSweep/Quarantine"));
    }
    match xdg {
        Some(x) if x.is_absolute() => Some(x.join("opensweep/quarantine")),
        _ => home.map(|h| h.join(".local/share/opensweep/quarantine")),
    }
}

#[derive(Debug, Clone)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    /// Opens the store at `root`, creating it owner-only if needed. The store
    /// and its `runs` folder must be real folders (not links) owned by the
    /// owner of the home folder, and no one else may change them (I19): a
    /// store planted by another user could steer restore and purge.
    pub fn open(root: &Path) -> Result<Self, Error> {
        let fail = |m: String| Error::Store(root.to_path_buf(), m);
        if let Some(parent) = root.parent() {
            fs::create_dir_all(parent).map_err(|e| fail(e.to_string()))?;
        }
        let owner = std::env::var_os("HOME")
            .map(PathBuf::from)
            .and_then(|h| owner_of(&h).ok());
        let owner = match owner {
            Some(o) => o,
            None => owner_of(root.parent().unwrap_or(root)).map_err(fail)?,
        };
        private_dir(root, owner, true).map_err(fail)?;
        private_dir(&root.join("runs"), owner, true).map_err(fail)?;
        let root = fs::canonicalize(root).map_err(|e| fail(e.to_string()))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn runs_dir(&self) -> PathBuf {
        self.root.join("runs")
    }

    fn run_dir(&self, run_id: &str) -> Result<PathBuf, Error> {
        if !valid_run_id(run_id) {
            return Err(Error::UnknownRun(run_id.to_owned()));
        }
        let dir = self.runs_dir().join(run_id);
        // Never through a link: a linked run would point restore and purge
        // somewhere else.
        let real_file = fs::symlink_metadata(dir.join(MANIFEST)).is_ok_and(|m| m.is_file());
        if !real_file || fs::symlink_metadata(&dir).map_or(true, |m| !m.is_dir()) {
            return Err(Error::UnknownRun(run_id.to_owned()));
        }
        Ok(dir)
    }

    /// Moves every approved directory into a new run. Items on another local
    /// drive go to that drive's quarantine. Items that changed since approval,
    /// or whose drive cannot hold a quarantine, are recorded as skipped and
    /// left where they are. Returns the final manifest.
    pub fn clean(&self, plan: Vec<Approved>) -> Result<Manifest, Error> {
        if plan.is_empty() {
            return Err(Error::EmptyPlan);
        }
        let store_dev =
            device_of(&self.root).map_err(|e| Error::Store(self.root.clone(), e.to_string()))?;
        let (run_id, run_dir) = self.new_run_dir()?;
        let items = run_dir.join("items");
        let io = |e: std::io::Error| Error::Io(run_id.clone(), e.to_string());
        owner_only().create(&items).map_err(io)?;

        let mut manifest = Manifest {
            format: FORMAT,
            run_id: run_id.clone(),
            created_unix: now_unix(),
            entries: plan
                .iter()
                .enumerate()
                .map(|(i, a)| Entry {
                    original: a.path().to_path_buf(),
                    project_root: a.project_root().to_path_buf(),
                    stored: format!("{i:04}"),
                    rule_id: a.rule_id().to_owned(),
                    class: a.class(),
                    bytes: size::usage(a.path()).bytes,
                    state: State::Pending,
                    volume: None,
                    note: None,
                })
                .collect(),
        };
        write_manifest(&run_dir, &manifest).map_err(io)?;

        let owner = owner_of(&self.root).map_err(|e| Error::Store(self.root.clone(), e))?;
        let mut mounts: Option<Option<Vec<Mount>>> = None;
        // Per device: the drive quarantine and this run's items folder there.
        let mut drives: HashMap<u64, Result<(PathBuf, PathBuf), String>> = HashMap::new();
        for (i, approved) in plan.iter().enumerate() {
            let target = if approved.device() == store_dev {
                Ok(items.clone())
            } else {
                let drive = drives.entry(approved.device()).or_insert_with(|| {
                    let mounts = mounts.get_or_insert_with(volumes::mounts).as_deref();
                    drive_run(approved.path(), &run_id, owner, mounts)
                });
                match drive {
                    Ok((store, items)) if volumes::device(store) == Some(approved.device()) => {
                        // Recorded before the move, so a crash cannot lose it.
                        manifest.entries[i].volume = Some(store.clone());
                        write_manifest(&run_dir, &manifest).map_err(io)?;
                        Ok(items.clone())
                    }
                    Ok(_) => Err("its drive's quarantine is on another drive".to_owned()),
                    Err(why) => Err(why.clone()),
                }
            };
            let outcome = match target {
                Err(why) => Err((State::Skipped, why)),
                Ok(items) => match approved.reverify() {
                    Err(why) => Err((State::Skipped, why.to_string())),
                    Ok(()) => {
                        let name = OsStr::new(&manifest.entries[i].stored);
                        approved.move_into(&items, name).map_err(|e| match e {
                            MoveFailed::Changed => {
                                (State::Skipped, Refusal::ChangedSinceApproval.to_string())
                            }
                            MoveFailed::Io(e) => (State::Failed, e.to_string()),
                        })
                    }
                },
            };
            let entry = &mut manifest.entries[i];
            match outcome {
                Ok(()) => entry.state = State::Moved,
                Err((state, note)) => {
                    entry.state = state;
                    entry.note = Some(note);
                }
            }
            write_manifest(&run_dir, &manifest).map_err(io)?;
        }
        Ok(manifest)
    }

    /// Every run, newest first. Fails if any manifest is unreadable, so that a
    /// damaged run is never silently hidden.
    pub fn runs(&self) -> Result<Vec<Manifest>, Error> {
        let dir = self.runs_dir();
        let entries = fs::read_dir(&dir).map_err(|e| Error::Store(dir.clone(), e.to_string()))?;
        let mut out = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| Error::Store(dir.clone(), e.to_string()))?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if !valid_run_id(&name) {
                continue;
            }
            out.push(self.load(&name)?);
        }
        out.sort_by(|a, b| {
            b.created_unix
                .cmp(&a.created_unix)
                .then(b.run_id.cmp(&a.run_id))
        });
        Ok(out)
    }

    pub fn load(&self, run_id: &str) -> Result<Manifest, Error> {
        let dir = self.run_dir(run_id)?;
        let text = fs::read_to_string(dir.join(MANIFEST))
            .map_err(|e| Error::BadManifest(run_id.to_owned(), e.to_string()))?;
        parse_manifest(run_id, &text)
    }

    /// Moves items of a run back where they came from: all of them, or the one
    /// at index `item`. Never overwrites: an item whose original location is
    /// occupied, or whose parent folder is gone, stays in quarantine with a note.
    pub fn restore(&self, run_id: &str, item: Option<usize>) -> Result<Manifest, Error> {
        let dir = self.run_dir(run_id)?;
        let mut manifest = self.load(run_id)?;
        let picked = selection(&manifest, run_id, item)?;
        let io = |e: std::io::Error| Error::Io(run_id.to_owned(), e.to_string());
        let owner = owner_of(&self.root).map_err(|e| Error::Store(self.root.clone(), e))?;
        let mounts = manifest
            .entries
            .iter()
            .any(|e| e.volume.is_some())
            .then(volumes::mounts)
            .flatten();

        for i in picked {
            let entry = &mut manifest.entries[i];
            let src = match items_of(&dir, run_id, entry, owner, mounts.as_deref()) {
                Ok(items) => items.join(&entry.stored),
                Err(why) => {
                    if entry.state == State::Moved {
                        entry.note = Some(why);
                    }
                    continue;
                }
            };
            if !held(entry, &src) {
                continue;
            }
            let dest = entry.original.clone();
            let outcome = if fs::symlink_metadata(&dest).is_ok() {
                Err("something already exists at the original location".to_owned())
            } else if !dest.parent().is_some_and(Path::is_dir) {
                Err("the folder it came from no longer exists".to_owned())
            } else if !dest
                .parent()
                .is_some_and(|p| fs::canonicalize(p).is_ok_and(|c| c == p))
            {
                // `original` was canonical at clean time; a different real path
                // now means a folder on the way was replaced, e.g. by a link.
                Err("the folder it came from is now a link or has moved".to_owned())
            } else {
                match (src.parent(), dest.parent(), dest.file_name()) {
                    (Some(from), Some(to), Some(name)) => {
                        rename_into(from, OsStr::new(&entry.stored), to, name)
                            .map_err(|e| e.to_string())
                    }
                    _ => Err("the folder it came from no longer exists".to_owned()),
                }
            };
            match outcome {
                Ok(()) => {
                    entry.state = State::Restored;
                    entry.note = None;
                }
                Err(note) => {
                    entry.state = State::Moved;
                    entry.note = Some(note);
                }
            }
        }
        write_manifest(&dir, &manifest).map_err(io)?;
        Ok(manifest)
    }

    /// Permanently deletes a run's quarantined items. Only paths inside this
    /// store's `items/` folder are ever deleted. The manifest is kept as history.
    pub fn purge(&self, run_id: &str) -> Result<Manifest, Error> {
        let dir = self.run_dir(run_id)?;
        let mut manifest = self.load(run_id)?;
        let io = |e: std::io::Error| Error::Io(run_id.to_owned(), e.to_string());
        let owner = owner_of(&self.root).map_err(|e| Error::Store(self.root.clone(), e))?;
        let mounts = manifest
            .entries
            .iter()
            .any(|e| e.volume.is_some())
            .then(volumes::mounts)
            .flatten();

        for entry in &mut manifest.entries {
            let target = match items_of(&dir, run_id, entry, owner, mounts.as_deref()) {
                Ok(items) => items.join(&entry.stored),
                Err(why) => {
                    if entry.state == State::Moved {
                        entry.note = Some(why);
                    }
                    continue;
                }
            };
            if !held(entry, &target) {
                continue;
            }
            match fs::symlink_metadata(&target) {
                // `symlink_metadata` never reports a link as a directory.
                Ok(m) if m.is_dir() => {
                    // `remove_dir_all` does not follow symbolic links.
                    match fs::remove_dir_all(&target) {
                        Ok(()) => {
                            entry.state = State::Purged;
                            entry.note = None;
                        }
                        Err(e) => entry.note = Some(e.to_string()),
                    }
                }
                _ => entry.note = Some("quarantined item is not a directory".to_owned()),
            }
        }
        write_manifest(&dir, &manifest).map_err(io)?;
        Ok(manifest)
    }

    /// Purges every run created at least `days` days ago. Returns the runs purged.
    pub fn purge_older_than(&self, days: u64) -> Result<Vec<Manifest>, Error> {
        let cutoff = now_unix().saturating_sub(days.saturating_mul(86_400));
        let mut out = Vec::new();
        for run in self.runs()? {
            if run.created_unix <= cutoff && run.bytes_held() > 0 {
                out.push(self.purge(&run.run_id)?);
            }
        }
        Ok(out)
    }

    fn new_run_dir(&self) -> Result<(String, PathBuf), Error> {
        let base = format!("{}-{}", utc_stamp(now_unix()), std::process::id());
        for n in 0..1000u32 {
            let id = if n == 0 {
                base.clone()
            } else {
                format!("{base}-{n}")
            };
            let dir = self.runs_dir().join(&id);
            match owner_only().create(&dir) {
                Ok(()) => return Ok((id, dir)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(Error::Store(self.root.clone(), e.to_string())),
            }
        }
        Err(Error::Store(
            self.root.clone(),
            "could not allocate a run folder".to_owned(),
        ))
    }
}

/// Parses and validates the manifest text of run `run_id`. Public so the fuzz
/// targets can reach it; every accepted entry names a plain item under
/// `items/` and an absolute original.
pub fn parse_manifest(run_id: &str, text: &str) -> Result<Manifest, Error> {
    let bad = |m: String| Error::BadManifest(run_id.to_owned(), m);
    let manifest: Manifest = serde_json::from_str(text).map_err(|e| bad(e.to_string()))?;
    if manifest.format != FORMAT {
        return Err(bad(format!("format {} is not supported", manifest.format)));
    }
    if manifest.run_id != run_id {
        return Err(bad("run id does not match its folder".to_owned()));
    }
    for e in &manifest.entries {
        let inside = absolute_plain(&e.project_root)
            && absolute_plain(&e.original)
            && e.original.starts_with(&e.project_root)
            && e.original != e.project_root;
        // A drive quarantine sits at the top of the drive the item came from.
        let volume_ok = e.volume.as_ref().map_or(true, |v| {
            absolute_plain(v)
                && v.file_name().is_some_and(|n| n == VOLUME_STORE)
                && v.parent().is_some_and(|d| e.original.starts_with(d))
        });
        if !valid_stored(&e.stored) || !inside || !volume_ok {
            return Err(bad(format!("entry {:?} is malformed", e.stored)));
        }
    }
    Ok(manifest)
}

/// Absolute, with no `.` or `..` components.
fn absolute_plain(path: &Path) -> bool {
    path.is_absolute()
        && path.components().all(|c| {
            matches!(
                c,
                Component::RootDir | Component::Prefix(_) | Component::Normal(_)
            )
        })
}

/// True if the entry's item is in quarantine. A `pending` entry whose item is
/// present was moved just before a crash; it is treated as moved.
fn held(entry: &mut Entry, item: &Path) -> bool {
    match entry.state {
        State::Moved => true,
        State::Pending if fs::symlink_metadata(item).is_ok() => {
            entry.state = State::Moved;
            true
        }
        _ => false,
    }
}

fn selection(manifest: &Manifest, run_id: &str, item: Option<usize>) -> Result<Vec<usize>, Error> {
    match item {
        None => Ok((0..manifest.entries.len()).collect()),
        Some(i) if i < manifest.entries.len() => Ok(vec![i]),
        Some(i) => Err(Error::UnknownItem(run_id.to_owned(), i)),
    }
}

/// Writes the manifest via a temporary file and a rename, so a crash leaves
/// either the old manifest or the new one, never half of one.
fn write_manifest(run_dir: &Path, manifest: &Manifest) -> std::io::Result<()> {
    let json = serde_json::to_vec_pretty(manifest).map_err(std::io::Error::other)?;
    let tmp = run_dir.join(format!("{MANIFEST}.tmp"));
    let mut file = fs::File::create(&tmp)?;
    file.write_all(&json)?;
    file.sync_all()?;
    fs::rename(&tmp, run_dir.join(MANIFEST))
}

fn valid_run_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn valid_stored(name: &str) -> bool {
    name.len() >= 4 && name.bytes().all(|b| b.is_ascii_digit())
}

/// The drive quarantine for the drive holding `path`, and a new items folder
/// for this run in it. Creates both, owner-only; refuses network drives and
/// a quarantine someone else could change (I19).
fn drive_run(
    path: &Path,
    run_id: &str,
    owner: u32,
    mounts: Option<&[Mount]>,
) -> Result<(PathBuf, PathBuf), String> {
    let drive = volumes::drive_root(path).ok_or("its drive could not be read")?;
    if let Some(why) = volumes::unusable(&drive, mounts) {
        return Err(why);
    }
    shared_safely(&drive)?;
    let store = drive.join(VOLUME_STORE);
    let run = store.join("runs").join(run_id);
    let items = run.join("items");
    let fail = |e: String| format!("no quarantine on its drive: {e}");
    private_dir(&store, owner, true).map_err(fail)?;
    private_dir(&store.join("runs"), owner, true).map_err(fail)?;
    private_dir(&run, owner, false).map_err(fail)?;
    private_dir(&items, owner, false).map_err(fail)?;
    Ok((store, items))
}

/// A folder builder that creates owner-only folders (0700).
fn owner_only() -> fs::DirBuilder {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
}

/// Creates `dir` owner-only (it must be new unless `existing_ok`), then
/// checks it is trusted.
fn private_dir(dir: &Path, owner: u32, existing_ok: bool) -> Result<(), String> {
    match owner_only().create(dir) {
        Ok(()) => {}
        Err(e) if existing_ok && e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(format!("{}: {e}", dir.display())),
    }
    trusted(dir, owner)
}

/// The top of a drive may be shared only with the sticky bit set, so no one
/// else can rename or replace the quarantine folder in it.
pub(crate) fn shared_safely(drive: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let mode = fs::metadata(drive)
            .map_err(|e| format!("{}: {e}", drive.display()))?
            .mode();
        if mode & 0o022 != 0 && mode & 0o1000 == 0 {
            return Err(format!(
                "the top of its drive ({}) can be changed by other users",
                drive.display()
            ));
        }
    }
    #[cfg(not(unix))]
    let _ = drive;
    Ok(())
}

/// I19: a real folder (not a link), owned by `owner`, that no one else can
/// change.
pub(crate) fn trusted(dir: &Path, owner: u32) -> Result<(), String> {
    let meta = fs::symlink_metadata(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    // `symlink_metadata` never reports a link as a directory.
    if !meta.is_dir() {
        return Err(format!("{} is not a real folder", dir.display()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.uid() != owner {
            return Err(format!("{} belongs to another user", dir.display()));
        }
        if meta.mode() & 0o022 != 0 {
            return Err(format!("{} can be changed by other users", dir.display()));
        }
    }
    #[cfg(not(unix))]
    let _ = owner;
    Ok(())
}

/// Where an entry's item is held: the main store's items folder, or the
/// drive quarantine's, checked again before every use.
fn items_of(
    run_dir: &Path,
    run_id: &str,
    entry: &Entry,
    owner: u32,
    mounts: Option<&[Mount]>,
) -> Result<PathBuf, String> {
    let Some(store) = &entry.volume else {
        let items = run_dir.join("items");
        trusted(run_dir, owner)?;
        trusted(&items, owner)?;
        return Ok(items);
    };
    if fs::symlink_metadata(store).is_err() {
        return Err(format!("its drive is not connected ({})", store.display()));
    }
    // The quarantine must still sit at the top of a local drive.
    let drive = store.parent().ok_or("its quarantine has no drive")?;
    if volumes::drive_root(store).as_deref() != Some(drive) {
        return Err(format!("{} is not at the top of a drive", store.display()));
    }
    if let Some(why) = volumes::unusable(drive, mounts) {
        return Err(why);
    }
    shared_safely(drive)?;
    let run = store.join("runs").join(run_id);
    let items = run.join("items");
    for dir in [store, &store.join("runs"), &run, &items] {
        trusted(dir, owner)?;
    }
    Ok(items)
}

fn owner_of(path: &Path) -> Result<u32, String> {
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    let uid = {
        use std::os::unix::fs::MetadataExt;
        meta.uid()
    };
    #[cfg(not(unix))]
    let uid = {
        let _ = meta;
        0
    };
    Ok(uid)
}

fn device_of(path: &Path) -> std::io::Result<u64> {
    let meta = fs::metadata(path)?;
    #[cfg(unix)]
    let dev = {
        use std::os::unix::fs::MetadataExt;
        meta.dev()
    };
    #[cfg(not(unix))]
    let dev = {
        let _ = meta;
        0
    };
    Ok(dev)
}

/// Seconds since 1970, or 0 if the clock is before it.
pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `YYYYMMDDTHHMMSSZ` for a Unix time.
pub fn utc_stamp(secs: u64) -> String {
    let (y, m, d) = civil_from_days(secs / 86_400);
    let rem = secs % 86_400;
    format!(
        "{y:04}{m:02}{d:02}T{:02}{:02}{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Days since 1970-01-01 to a proleptic Gregorian date (Howard Hinnant's
/// `civil_from_days`), for dates from 1970 on, so no negative era.
pub fn civil_from_days(days: u64) -> (u64, u32, u32) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + u64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_known_dates() {
        assert_eq!(utc_stamp(0), "19700101T000000Z");
        assert_eq!(utc_stamp(951_782_400), "20000229T000000Z");
        assert_eq!(utc_stamp(1_795_086_508), "20261119T110828Z");
    }

    #[test]
    fn dates_match_day_by_day_counting_to_2500() {
        let leap = |y: u64| (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
        let (mut y, mut m, mut d) = (1970u64, 1u32, 1u32);
        for days in 0..(531 * 366) {
            assert_eq!(civil_from_days(days), (y, m, d), "day {days}");
            let len = match m {
                2 if leap(y) => 29,
                2 => 28,
                4 | 6 | 9 | 11 => 30,
                _ => 31,
            };
            d += 1;
            if d > len {
                d = 1;
                m += 1;
                if m > 12 {
                    m = 1;
                    y += 1;
                }
            }
        }
    }

    #[test]
    fn store_location_per_platform() {
        let home = Some(PathBuf::from("/home/u"));
        let at = |s: &str| Some(PathBuf::from(s));
        assert_eq!(
            location_for(true, home.clone(), at("/x")),
            at("/home/u/Library/Application Support/OpenSweep/Quarantine")
        );
        assert_eq!(
            location_for(false, home.clone(), at("/x")),
            at("/x/opensweep/quarantine")
        );
        // A relative XDG_DATA_HOME is ignored, as the XDG spec says.
        for xdg in [at("rel"), None] {
            assert_eq!(
                location_for(false, home.clone(), xdg),
                at("/home/u/.local/share/opensweep/quarantine")
            );
        }
        assert_eq!(location_for(true, None, None), None);
    }

    #[test]
    fn run_ids_and_item_names_are_plain() {
        assert!(valid_run_id("20261119T094828Z-42-1"));
        for bad in ["", "..", "a/b", "a b", "x\0"] {
            assert!(!valid_run_id(bad), "{bad:?}");
        }
        assert!(valid_stored("0007"));
        for bad in ["7", "../0001", "0001/x", "abcd"] {
            assert!(!valid_stored(bad), "{bad:?}");
        }
    }
}
