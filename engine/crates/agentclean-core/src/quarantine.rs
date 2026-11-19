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

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::guard::Approved;
use crate::rules::Class;
use crate::size;

pub const MANIFEST: &str = "manifest.json";
pub const FORMAT: u32 = 1;

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
/// macOS: `~/Library/Application Support/AgentClean/Quarantine`.
/// Elsewhere: `$XDG_DATA_HOME/agentclean/quarantine`, or
/// `~/.local/share/agentclean/quarantine`.
pub fn default_location() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(target_os = "macos") {
        return home.map(|h| h.join("Library/Application Support/AgentClean/Quarantine"));
    }
    match std::env::var_os("XDG_DATA_HOME").map(PathBuf::from) {
        Some(x) if x.is_absolute() => Some(x.join("agentclean/quarantine")),
        _ => home.map(|h| h.join(".local/share/agentclean/quarantine")),
    }
}

#[derive(Debug, Clone)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    /// Opens the store at `root`, creating it if needed. The store itself may
    /// not be a symbolic link.
    pub fn open(root: &Path) -> Result<Self, Error> {
        let fail = |m: String| Error::Store(root.to_path_buf(), m);
        fs::create_dir_all(root.join("runs")).map_err(|e| fail(e.to_string()))?;
        let meta = fs::symlink_metadata(root).map_err(|e| fail(e.to_string()))?;
        if meta.file_type().is_symlink() || !meta.is_dir() {
            return Err(fail("must be a real directory".to_owned()));
        }
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
        if !dir.join(MANIFEST).is_file() {
            return Err(Error::UnknownRun(run_id.to_owned()));
        }
        Ok(dir)
    }

    /// Moves every approved directory into a new run. Items that changed since
    /// approval, or sit on another volume, are recorded as skipped and left
    /// where they are. Returns the final manifest.
    pub fn clean(&self, plan: Vec<Approved>) -> Result<Manifest, Error> {
        if plan.is_empty() {
            return Err(Error::EmptyPlan);
        }
        let store_dev =
            device_of(&self.root).map_err(|e| Error::Store(self.root.clone(), e.to_string()))?;
        let (run_id, run_dir) = self.new_run_dir()?;
        let items = run_dir.join("items");
        let io = |e: std::io::Error| Error::Io(run_id.clone(), e.to_string());
        fs::create_dir(&items).map_err(io)?;

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
                    note: None,
                })
                .collect(),
        };
        write_manifest(&run_dir, &manifest).map_err(io)?;

        for (i, approved) in plan.iter().enumerate() {
            let outcome = if approved.device() != store_dev {
                Err((
                    State::Skipped,
                    "on a different volume from the quarantine".to_owned(),
                ))
            } else if let Err(why) = approved.reverify() {
                Err((State::Skipped, why.to_string()))
            } else {
                let dest = items.join(&manifest.entries[i].stored);
                fs::rename(approved.path(), &dest).map_err(|e| (State::Failed, e.to_string()))
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
        let bad = |m: String| Error::BadManifest(run_id.to_owned(), m);
        let text = fs::read_to_string(dir.join(MANIFEST)).map_err(|e| bad(e.to_string()))?;
        let manifest: Manifest = serde_json::from_str(&text).map_err(|e| bad(e.to_string()))?;
        if manifest.format != FORMAT {
            return Err(bad(format!("format {} is not supported", manifest.format)));
        }
        if manifest.run_id != run_id {
            return Err(bad("run id does not match its folder".to_owned()));
        }
        for e in &manifest.entries {
            if !valid_stored(&e.stored) || !e.original.is_absolute() {
                return Err(bad(format!("entry {:?} is malformed", e.stored)));
            }
        }
        Ok(manifest)
    }

    /// Moves items of a run back where they came from: all of them, or the one
    /// at index `item`. Never overwrites: an item whose original location is
    /// occupied, or whose parent folder is gone, stays in quarantine with a note.
    pub fn restore(&self, run_id: &str, item: Option<usize>) -> Result<Manifest, Error> {
        let dir = self.run_dir(run_id)?;
        let mut manifest = self.load(run_id)?;
        let picked = selection(&manifest, run_id, item)?;
        let io = |e: std::io::Error| Error::Io(run_id.to_owned(), e.to_string());

        for i in picked {
            let entry = &mut manifest.entries[i];
            let src = dir.join("items").join(&entry.stored);
            if !held(entry, &src) {
                continue;
            }
            let dest = entry.original.clone();
            let outcome = if fs::symlink_metadata(&dest).is_ok() {
                Err("something already exists at the original location".to_owned())
            } else if !dest.parent().is_some_and(Path::is_dir) {
                Err("the folder it came from no longer exists".to_owned())
            } else {
                fs::rename(&src, &dest).map_err(|e| e.to_string())
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
        let items = dir.join("items");
        let io = |e: std::io::Error| Error::Io(run_id.to_owned(), e.to_string());

        for entry in &mut manifest.entries {
            let target = items.join(&entry.stored);
            if !held(entry, &target) {
                continue;
            }
            match fs::symlink_metadata(&target) {
                Ok(m) if m.is_dir() && !m.file_type().is_symlink() => {
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
            match fs::create_dir(&dir) {
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

#[cfg(unix)]
fn device_of(path: &Path) -> std::io::Result<u64> {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(path).map(|m| m.dev())
}

#[cfg(not(unix))]
fn device_of(_path: &Path) -> std::io::Result<u64> {
    Ok(0)
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `YYYYMMDDTHHMMSSZ` for a Unix time.
pub fn utc_stamp(secs: u64) -> String {
    let (y, m, d) = civil_from_days((secs / 86_400) as i64);
    let rem = secs % 86_400;
    format!(
        "{y:04}{m:02}{d:02}T{:02}{:02}{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Days since 1970-01-01 to a proleptic Gregorian date (Howard Hinnant's
/// `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
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
