//! Is a project in use? Three signals: the last commit touching it, the
//! newest edit to a file that is not build output, and tools running inside
//! it. Pinned projects are never cleaned, whatever the signals say.
//!
//! An [`Activity`] can only be made here, and [`crate::guard::Guard::check`]
//! takes one, so nothing is approved without an activity assessment (I15).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::discover::not_walked;
use crate::guard::{hardened_git, inside_git_checkout};
use crate::rules::RuleFile;

/// Files and folders read per project when looking for the newest edit.
const MAX_WALK_ENTRIES: usize = 20_000;
const MAX_WALK_DEPTH: usize = 16;
const DAY: u64 = 86_400;

/// `config.toml`: thresholds and pins. Read strictly: an unknown key or a
/// relative pin is an error, so a typo cannot silently drop a pin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    /// Changed within this many days: Active.
    pub idle_days: u64,
    /// Unchanged for at least this many days: Dormant.
    pub dormant_days: u64,
    /// Never cleaned, and nor is anything inside them.
    pub pinned: Vec<PathBuf>,
    /// Folders `scan` does not look in.
    pub exclude: Vec<PathBuf>,
    /// A pin in `pinned` ends at this time (Unix seconds).
    pub pinned_until: BTreeMap<PathBuf, u64>,
    /// Repositories pinned wherever they are cloned: normalised `origin`
    /// remotes (`host/owner/repo`).
    pub pinned_remotes: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            idle_days: 14,
            dormant_days: 90,
            pinned: Vec::new(),
            exclude: Vec::new(),
            pinned_until: BTreeMap::new(),
            pinned_remotes: Vec::new(),
        }
    }
}

impl Config {
    /// A missing file is the default config; anything else unreadable or
    /// invalid is an error, never a silent default.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        let cfg: Self = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        if let Some(p) = cfg.exclude.iter().find(|p| !p.is_absolute()) {
            return Err(format!(
                "{}: exclusion {} is not absolute",
                path.display(),
                p.display()
            ));
        }
        if let Some(p) = cfg.pinned.iter().find(|p| !p.is_absolute()) {
            return Err(format!(
                "{}: pin {} is not absolute",
                path.display(),
                p.display()
            ));
        }
        if cfg.idle_days > cfg.dormant_days {
            return Err(format!(
                "{}: idle_days is above dormant_days",
                path.display()
            ));
        }
        Ok(cfg)
    }

    /// Writes atomically: a crash leaves the old file or the new one.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let text = toml::to_string(self).map_err(|e| e.to_string())?;
        let err = |e: std::io::Error| format!("{}: {e}", path.display());
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(err)?;
        }
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text).map_err(err)?;
        std::fs::rename(&tmp, path).map_err(err)
    }

    fn pinned(&self, root: &Path, now: SystemTime) -> bool {
        let now = now
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let by_path = self.pinned.iter().any(|p| {
            let live = self.pinned_until.get(p).map_or(true, |&until| until > now);
            let p = std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());
            live && root.starts_with(p)
        });
        by_path
            || (!self.pinned_remotes.is_empty()
                && git_remote(root).is_some_and(|r| {
                    self.pinned_remotes
                        .iter()
                        .any(|p| normalise_remote(p).as_deref() == Some(r.as_str()))
                }))
    }
}

/// A repository's own `.opensweep.toml`. It may only take folders away
/// from cleaning: pin the project, or keep folders.
struct RepoFile {
    pin: bool,
    exclude: Vec<PathBuf>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RepoToml {
    #[serde(default)]
    pin: bool,
    #[serde(default)]
    exclude: Vec<String>,
}

/// Reads `<root>/.opensweep.toml`: Ok(None) when there is none, Err when it
/// says anything but a plain pin or relative folders (the caller pins).
fn repo_file(root: &Path) -> Result<Option<RepoFile>, String> {
    let path = root.join(".opensweep.toml");
    let meta = match std::fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("cannot be read".to_owned()),
    };
    if !meta.is_file() || meta.len() > 64 * 1024 {
        return Err("is not a small plain file".to_owned());
    }
    let text = std::fs::read_to_string(&path).map_err(|_| "cannot be read".to_owned())?;
    let raw: RepoToml = toml::from_str(&text).map_err(|_| "is not pin or exclude".to_owned())?;
    let mut exclude = Vec::new();
    for e in raw.exclude {
        let p = PathBuf::from(e.trim_end_matches('/'));
        let plain = p.components().count() > 0
            && p.components()
                .all(|c| matches!(c, std::path::Component::Normal(_)));
        if !plain {
            return Err(format!(
                "excludes {e:?}, which is not a folder inside the project"
            ));
        }
        exclude.push(p);
    }
    Ok(Some(RepoFile {
        pin: raw.pin,
        exclude,
    }))
}

/// The `origin` remote of the repository at `root`, normalised, read from
/// `.git/config` as text: git is not run, and a repository can only make
/// itself pinned, never unpin another.
pub fn git_remote(root: &Path) -> Option<String> {
    let path = root.join(".git/config");
    let meta = std::fs::symlink_metadata(&path).ok()?;
    if !meta.is_file() || meta.len() > 256 * 1024 {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    let mut in_origin = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_origin = line.replace(' ', "") == "[remote\"origin\"]";
        } else if in_origin {
            if let Some((k, v)) = line.split_once('=') {
                if k.trim() == "url" {
                    return normalise_remote(v.trim());
                }
            }
        }
    }
    None
}

/// `git@host:owner/repo.git`, `https://user@host/owner/repo/` and
/// `ssh://git@host/owner/repo` all become `host/owner/repo` (host lowercased).
pub fn normalise_remote(url: &str) -> Option<String> {
    let url = url.trim();
    let rest = match url.split_once("://") {
        Some((_, rest)) => rest.to_owned(),
        // scp-like: user@host:path
        None => url.replacen(
            ':',
            "/",
            usize::from(url.contains('@') || !url.starts_with('/')),
        ),
    };
    let rest = rest.rsplit_once('@').map_or(rest.as_str(), |(_, r)| r);
    let rest = rest
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .trim_end_matches('/');
    let (host, path) = rest.split_once('/')?;
    let host = host.split(':').next().unwrap_or(host).to_lowercase();
    (!host.is_empty() && !path.is_empty()).then(|| format!("{host}/{path}"))
}

/// Where `config.toml` lives by default.
pub fn default_config_path() -> Option<PathBuf> {
    let path = config_path_for(
        cfg!(target_os = "macos"),
        std::env::var_os("HOME").map(PathBuf::from),
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
    );
    if let Some(dir) = path.as_deref().and_then(Path::parent) {
        crate::quarantine::adopt_legacy(dir);
    }
    path
}

fn config_path_for(macos: bool, home: Option<PathBuf>, xdg: Option<PathBuf>) -> Option<PathBuf> {
    if macos {
        return home.map(|h| h.join("Library/Application Support/OpenSweep/config.toml"));
    }
    match xdg {
        Some(x) if x.is_absolute() => Some(x.join("opensweep/config.toml")),
        _ => home.map(|h| h.join(".config/opensweep/config.toml")),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Active,
    Idle,
    Dormant,
}

/// The assessment of one project. Private fields: only [`assess`] makes one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activity {
    root: PathBuf,
    verdict: Verdict,
    pinned: bool,
    reason: String,
    /// Folders the repository's `.opensweep.toml` keeps, relative.
    kept: Vec<PathBuf>,
}

impl Activity {
    /// The project root, as given to [`assess`].
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn verdict(&self) -> Verdict {
        self.verdict
    }

    pub fn pinned(&self) -> bool {
        self.pinned
    }

    /// Folders, relative to the root, that the repository asks never to clean.
    pub fn kept(&self) -> &[PathBuf] {
        &self.kept
    }

    #[cfg(test)]
    pub(crate) fn keeping(mut self, kept: Vec<PathBuf>) -> Self {
        self.kept = kept;
        self
    }

    /// One plain-English line saying why.
    pub fn reason(&self) -> &str {
        &self.reason
    }

    #[cfg(test)]
    pub(crate) fn unchecked(root: &Path, verdict: Verdict, pinned: bool) -> Self {
        Self {
            root: root.to_path_buf(),
            verdict,
            pinned,
            reason: "test".to_owned(),
            kept: Vec::new(),
        }
    }
}

/// A running process and the folder it runs in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Process {
    pub pid: u32,
    pub ppid: u32,
    /// Process group: a shell pipeline (`opensweep plan | less`) shares one.
    pub pgid: u32,
    pub name: String,
    pub cwd: PathBuf,
}

/// Processes running when the snapshot was taken, without this process, its
/// children, its ancestors (the shell and terminal it was started from), or
/// the rest of its pipeline.
#[derive(Debug, Clone, Default)]
pub struct Processes(Vec<Process>);

impl Processes {
    /// None if the running processes could not be listed.
    pub fn snapshot() -> Option<Self> {
        Some(Self::others(list_processes()?, std::process::id()))
    }

    fn others(all: Vec<Process>, me: u32) -> Self {
        let parent: HashMap<u32, u32> = all.iter().map(|p| (p.pid, p.ppid)).collect();
        let group = all.iter().find(|p| p.pid == me).map_or(0, |p| p.pgid);
        let mut mine = HashSet::from([me]);
        let mut at = me;
        while let Some(&up) = parent.get(&at) {
            if up == 0 || !mine.insert(up) {
                break;
            }
            at = up;
        }
        Self(
            all.into_iter()
                .filter(|p| !mine.contains(&p.pid) && p.ppid != me)
                .filter(|p| group == 0 || p.pgid != group)
                .collect(),
        )
    }

    fn inside(&self, root: &Path) -> Option<&Process> {
        self.0.iter().find(|p| p.cwd.starts_with(root))
    }
}

#[cfg(target_os = "linux")]
fn list_processes() -> Option<Vec<Process>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir("/proc").ok()?.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|n| n.parse().ok()) else {
            continue;
        };
        let dir = entry.path();
        // Other users' processes: cwd is unreadable, which is fine.
        let Ok(cwd) = std::fs::read_link(dir.join("cwd")) else {
            continue;
        };
        let stat = std::fs::read_to_string(dir.join("stat")).unwrap_or_default();
        // "pid (name) state ppid ...": the name may hold spaces and ')'.
        // After the name: state, ppid, pgrp.
        let field = |n| {
            stat.rsplit_once(')')
                .and_then(|(_, rest)| rest.split_whitespace().nth(n)?.parse().ok())
                .unwrap_or(0)
        };
        let (ppid, pgid) = (field(1), field(2));
        let name = std::fs::read_to_string(dir.join("comm")).unwrap_or_default();
        out.push(Process {
            pid,
            ppid,
            pgid,
            name: name.trim().to_owned(),
            cwd,
        });
    }
    Some(out)
}

#[cfg(not(target_os = "linux"))]
fn list_processes() -> Option<Vec<Process>> {
    // A fixed path and fixed arguments: no shell, no PATH lookup.
    use std::process::{Command, Stdio};
    let out = Command::new("/usr/sbin/lsof")
        .args(["-n", "-P", "-w", "-d", "cwd", "-F", "pgRcn"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let list = parse_lsof(&text);
    (!list.is_empty()).then_some(list)
}

/// `lsof -F pgRcn` output: one field per line, tagged by its first letter.
#[cfg_attr(target_os = "linux", allow(dead_code))]
fn parse_lsof(text: &str) -> Vec<Process> {
    let mut out = Vec::new();
    let mut cur: Option<Process> = None;
    for line in text.lines() {
        let (tag, value) = line.split_at(line.len().min(1));
        match tag {
            "p" => {
                out.extend(cur.take().filter(|p| p.cwd.is_absolute()));
                cur = value.parse().ok().map(|pid| Process {
                    pid,
                    ppid: 0,
                    pgid: 0,
                    name: String::new(),
                    cwd: PathBuf::new(),
                });
            }
            "R" => cur
                .iter_mut()
                .for_each(|p| p.ppid = value.parse().unwrap_or(0)),
            "g" => cur
                .iter_mut()
                .for_each(|p| p.pgid = value.parse().unwrap_or(0)),
            "c" => cur.iter_mut().for_each(|p| p.name = value.to_owned()),
            "n" => cur.iter_mut().for_each(|p| p.cwd = PathBuf::from(value)),
            _ => {}
        }
    }
    out.extend(cur.filter(|p| p.cwd.is_absolute()));
    out
}

/// Assesses one project. `processes` is None when they could not be listed.
pub fn assess(
    root: &Path,
    rules: &[RuleFile],
    cfg: &Config,
    processes: Option<&Processes>,
    now: SystemTime,
) -> Activity {
    let canon = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let commit = last_commit(&canon);
    let (edit, walked) = newest_edit(&canon, rules);
    let busy = processes.map(|p| p.inside(&canon));
    let mut activity = decide(cfg, now, commit, edit, walked, busy);
    activity.root = root.to_path_buf();
    if cfg.pinned(&canon, now) {
        activity.pinned = true;
        activity.reason = format!("pinned; {}", activity.reason);
    } else {
        match repo_file(&canon) {
            Ok(None) => {}
            Ok(Some(f)) if f.pin => {
                activity.pinned = true;
                activity.reason = format!("pinned by its .opensweep.toml; {}", activity.reason);
            }
            Ok(Some(f)) => activity.kept = f.exclude,
            Err(why) => {
                activity.pinned = true;
                activity.reason = format!("pinned: its .opensweep.toml {why}; {}", activity.reason);
            }
        }
    }
    activity
}

/// The verdict from the signals. `commit` is Err when git failed in a
/// repository; `walked` is false when the edit walk hit its limit; `busy` is
/// None when processes could not be listed.
fn decide(
    cfg: &Config,
    now: SystemTime,
    commit: Result<Option<SystemTime>, ()>,
    edit: Option<SystemTime>,
    walked: bool,
    busy: Option<Option<&Process>>,
) -> Activity {
    let make = |verdict, reason: String| Activity {
        root: PathBuf::new(),
        verdict,
        pinned: false,
        reason,
        kept: Vec::new(),
    };
    if let Some(Some(p)) = busy {
        return make(
            Verdict::Active,
            format!("{} (pid {}) is running in this folder", p.name, p.pid),
        );
    }
    let days = |t: SystemTime| now.duration_since(t).unwrap_or(Duration::ZERO).as_secs() / DAY;
    let mut seen = Vec::new();
    if let Some(t) = edit {
        seen.push((days(t), "edited"));
    }
    if let Ok(Some(t)) = commit {
        seen.push((days(t), "last commit"));
    }
    let newest = seen.iter().min_by_key(|(d, _)| *d).copied();
    let mut unsure = Vec::new();
    if commit.is_err() {
        unsure.push("git could not be read");
    }
    if !walked {
        unsure.push("too many files to check them all");
    }
    if busy.is_none() {
        unsure.push("running tools could not be checked");
    }
    let note = if unsure.is_empty() {
        String::new()
    } else {
        format!(" ({})", unsure.join("; "))
    };
    match newest {
        Some((d, what)) if d < cfg.idle_days => {
            make(Verdict::Active, format!("{what} {}{note}", ago(d)))
        }
        Some((d, what)) if d >= cfg.dormant_days && unsure.is_empty() => {
            make(Verdict::Dormant, format!("{what} {}", ago(d)))
        }
        Some((d, what)) => make(Verdict::Idle, format!("{what} {}{note}", ago(d))),
        None => make(Verdict::Idle, format!("no edits or commits found{note}")),
    }
}

fn ago(days: u64) -> String {
    match days {
        0 => "today".to_owned(),
        1 => "1 day ago".to_owned(),
        d => format!("{d} days ago"),
    }
}

/// Time of the last commit touching `root`: Ok(None) outside a repository or
/// with no such commit, Err if git failed in a repository.
fn last_commit(root: &Path) -> Result<Option<SystemTime>, ()> {
    if !inside_git_checkout(root) {
        return Ok(None);
    }
    let out = hardened_git(root)
        .args(["log", "-1", "--format=%ct", "--", "."])
        .output()
        .map_err(|_| ())?;
    if !out.status.success() {
        return Err(());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    match text.trim() {
        "" => Ok(None),
        // The repository chooses this number: a time past what SystemTime
        // holds is an error, not a panic that would end the whole scan.
        secs => secs
            .parse()
            .ok()
            .and_then(|s| UNIX_EPOCH.checked_add(Duration::from_secs(s)))
            .map(Some)
            .ok_or(()),
    }
}

/// Newest modification time of a file below `root`, skipping links, hidden
/// folders, bundles and the artifact folders of `root` and of every project
/// nested in it. The bool is false if the walk stopped at its limits.
fn newest_edit(root: &Path, rules: &[RuleFile]) -> (Option<SystemTime>, bool) {
    let mut artifacts = HashSet::new();
    let mut newest = None;
    let mut entries = 0usize;
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        for rule in rules.iter().filter(|r| r.detects(&dir)) {
            artifacts.extend(rule.artifacts.iter().map(|a| dir.join(&a.path)));
        }
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            entries += 1;
            if entries > MAX_WALK_ENTRIES {
                return (newest, false);
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_file() {
                let modified = entry.metadata().and_then(|m| m.modified()).ok();
                newest = newest.max(modified);
            } else if kind.is_dir() {
                let name = entry.file_name();
                let skip = name.to_str().map_or(true, not_walked) || artifacts.contains(&path);
                if skip {
                    continue;
                }
                if depth + 1 >= MAX_WALK_DEPTH {
                    return (newest, false);
                }
                stack.push((path, depth + 1));
            }
        }
    }
    (newest, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32, ppid: u32, cwd: &str) -> Process {
        Process {
            pid,
            ppid,
            pgid: pid,
            name: format!("p{pid}"),
            cwd: PathBuf::from(cwd),
        }
    }

    #[test]
    fn our_own_process_family_is_not_activity() {
        let all = vec![
            proc(1, 0, "/"),
            proc(10, 1, "/w/proj"),  // terminal
            proc(11, 10, "/w/proj"), // shell
            proc(12, 11, "/w/proj"), // opensweep (me)
            proc(13, 12, "/w/proj"), // lsof, my child
            proc(20, 11, "/w/proj"), // dev server started from the same shell
            proc(30, 1, "/w/other"),
        ];
        let others = Processes::others(all, 12);
        let pids: Vec<u32> = others.0.iter().map(|p| p.pid).collect();
        assert_eq!(pids, [20, 30]);
        assert_eq!(others.inside(Path::new("/w/proj")).map(|p| p.pid), Some(20));
        assert_eq!(others.inside(Path::new("/w/none")), None);
        // A parent loop does not hang.
        let looped = Processes::others(vec![proc(5, 6, "/"), proc(6, 5, "/")], 5);
        assert!(looped.0.is_empty());
        // The rest of our pipeline (`opensweep plan | less`) is ours too.
        let me = proc(12, 11, "/w/proj");
        let mut less = proc(14, 11, "/w/proj");
        less.pgid = 12;
        let server = proc(20, 11, "/w/proj");
        let others = Processes::others(vec![server, me, less], 12);
        assert_eq!(others.0.iter().map(|p| p.pid).collect::<Vec<_>>(), [20]);
    }

    #[test]
    fn lsof_output_is_parsed() {
        let text = "p1\ng1\nR0\nclaunchd\nfcwd\nn/\np42\nR7\ncnode\nfcwd\nn/w/my proj\np43\nR1\ncx\nfcwd\nnrelative\npbad\nn/x\n";
        let mut launchd = proc(1, 0, "/");
        launchd.name = "launchd".to_owned();
        let mut node = proc(42, 7, "/w/my proj");
        node.name = "node".to_owned();
        node.pgid = 0;
        assert_eq!(parse_lsof(text), [launchd, node]);
        assert!(parse_lsof("").is_empty());
    }

    #[test]
    fn config_location_per_platform() {
        let home = Some(PathBuf::from("/h"));
        assert_eq!(
            config_path_for(true, home.clone(), Some("/x".into())),
            Some("/h/Library/Application Support/OpenSweep/config.toml".into())
        );
        assert_eq!(
            config_path_for(false, home.clone(), Some("/x".into())),
            Some("/x/opensweep/config.toml".into())
        );
        assert_eq!(
            config_path_for(false, home, Some("rel".into())),
            Some("/h/.config/opensweep/config.toml".into())
        );
        assert_eq!(config_path_for(false, None, None), None);
    }
}
