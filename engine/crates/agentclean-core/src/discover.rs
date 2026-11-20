//! Finds projects below scan roots. Read-only: it reports projects and the
//! candidates their rules name; cleaning still goes through the guard.
//!
//! The walk never follows a link, never leaves the root's volume, never
//! enters the quarantine store, and never enters an artifact folder of a
//! project it found, so a package inside `node_modules` is not a project.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::guard::{file_id, installed_software};
use crate::rules::{Candidate, RuleFile};

/// Folders directly in the home folder that are never scanned.
const HOME_SKIPS: &[&str] = &[
    "Library",
    ".Trash",
    "Pictures",
    "Music",
    "Movies",
    "Applications",
    "go/pkg",
];

/// Folder suffixes of macOS bundles: packaged, not projects to walk into.
const BUNDLES: &[&str] = &[".app", ".xcodeproj", ".xcworkspace"];

/// Hidden folders and bundles: never walked into.
pub(crate) fn not_walked(name: &str) -> bool {
    name.starts_with('.') || BUNDLES.iter().any(|b| name.ends_with(b))
}

/// Top folders of the system disk that are never whole-disk roots: the
/// sealed system, other drives (`/Volumes`) and OS folders (only `usr/local` and `private/tmp`
/// are taken from `usr` and `private`).
const SYSTEM_TOP: &[&str] = &["System", "Volumes", "private", "usr", "cores", "dev"];

/// Every folder on this computer's own disk where projects can live, so one
/// scan covers the whole disk. Each is walked on its own volume, so I18 holds:
/// on macOS `/` is a sealed system volume and your files sit on a separate data
/// volume reached through `/Users`, `/Applications`, `/opt` and the like; those
/// top folders are the roots. With no separate data volume the root is `/`.
pub fn whole_disk_roots() -> Vec<PathBuf> {
    whole_disk_roots_in(Path::new("/"), Path::new("/System/Volumes/Data"))
}

pub(crate) fn whole_disk_roots_in(top: &Path, data: &Path) -> Vec<PathBuf> {
    let Ok(data) = std::fs::metadata(data) else {
        return vec![top.to_path_buf()];
    };
    let dev = file_id(&data).dev;
    // symlink_metadata: a link is never a root.
    let on_data =
        |p: &Path| std::fs::symlink_metadata(p).is_ok_and(|m| m.is_dir() && file_id(&m).dev == dev);
    let mut names: Vec<PathBuf> = std::fs::read_dir(top)
        .map(|rd| rd.filter_map(Result::ok).map(|e| e.path()).collect())
        .unwrap_or_default();
    names.sort();
    let mut roots: Vec<PathBuf> = names
        .into_iter()
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| !n.starts_with('.') && !SYSTEM_TOP.contains(&n))
        })
        .filter(|p| on_data(p))
        .collect();
    roots.extend(
        ["usr/local", "private/tmp"]
            .map(|r| top.join(r))
            .into_iter()
            .filter(|p| on_data(p)),
    );
    roots
}

#[derive(Debug, Clone)]
pub struct Options {
    pub roots: Vec<PathBuf>,
    /// Levels below a root that are entered; the root is level 0.
    pub max_depth: usize,
    /// Directory entries read per root before the walk of that root stops.
    pub max_entries: usize,
    /// Folders the user does not want scanned.
    pub exclude: Vec<PathBuf>,
    /// The quarantine store, never scanned.
    pub store: Option<PathBuf>,
    /// The home folder, whose [`HOME_SKIPS`] are not scanned.
    pub home: Option<PathBuf>,
}

impl Options {
    pub fn new(roots: Vec<PathBuf>) -> Self {
        Self {
            roots,
            max_depth: 16,
            max_entries: 2_000_000,
            exclude: Vec::new(),
            store: None,
            home: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Skip {
    Unreadable(String),
    OtherVolume,
    DepthLimit,
    EntryLimit,
    Excluded,
    /// Installed software (I21): nothing in it is ever cleaned.
    Installed(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A folder one or more rules detect, with the candidates not already
    /// reported for an enclosing project.
    Project {
        root: PathBuf,
        rules: Vec<String>,
        candidates: Vec<Candidate>,
    },
    Skipped {
        path: PathBuf,
        reason: Skip,
    },
}

/// Walks every root depth-first in name order, parents before children, and
/// reports each project and each folder it could not or would not scan.
// ponytail: single thread, and each rule stats its markers per folder; time it
// on a real home folder and parallelise or match names in memory if slow.
pub fn discover(rules: &[RuleFile], opts: &Options, mut on: impl FnMut(Event)) {
    let canon = |p: &PathBuf| std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());
    let excluded: HashSet<PathBuf> = opts.exclude.iter().map(canon).collect();
    let mut silent: HashSet<PathBuf> = opts.store.iter().map(canon).collect();
    let home = opts.home.as_ref().map(canon);
    if let Some(home) = &home {
        silent.extend(HOME_SKIPS.iter().map(|s| home.join(s)));
    }
    let mut visited = HashSet::new();
    let mut claimed = HashSet::new();

    for root in &opts.roots {
        // A file as root is reported by read_dir below ("Not a directory").
        let (root, meta) =
            match std::fs::canonicalize(root).and_then(|r| std::fs::metadata(&r).map(|m| (r, m))) {
                Ok(found) => found,
                Err(e) => {
                    let reason = Skip::Unreadable(e.to_string());
                    on(Event::Skipped {
                        path: root.clone(),
                        reason,
                    });
                    continue;
                }
            };
        let id = file_id(&meta);
        if !visited.insert((id.dev, id.ino)) {
            continue;
        }
        if let Some(what) = installed_software(&root, home.as_deref()) {
            on(Event::Skipped {
                path: root,
                reason: Skip::Installed(what),
            });
            continue;
        }
        let mut stack = vec![(root, 0usize)];
        let mut entries = 0usize;
        'walk: while let Some((dir, depth)) = stack.pop() {
            if !report_project(rules, &dir, &mut claimed, &mut on) {
                continue;
            }
            let mut children = match std::fs::read_dir(&dir) {
                Ok(read) => read.filter_map(Result::ok).collect::<Vec<_>>(),
                Err(e) => {
                    let reason = Skip::Unreadable(e.to_string());
                    on(Event::Skipped { path: dir, reason });
                    continue;
                }
            };
            children.sort_by_key(|e| e.file_name());
            let mut next = Vec::new();
            for entry in children {
                entries += 1;
                if entries > opts.max_entries {
                    on(Event::Skipped {
                        path: dir,
                        reason: Skip::EntryLimit,
                    });
                    break 'walk;
                }
                // DirEntry::file_type does not follow links: a link is never a dir here.
                if !entry.file_type().is_ok_and(|t| t.is_dir()) {
                    continue;
                }
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                if not_walked(name) {
                    continue;
                }
                let path = entry.path();
                if claimed.contains(&path) || silent.contains(&path) {
                    continue;
                }
                if let Some(what) = installed_software(&path, home.as_deref()) {
                    // Only where it starts: a stray node_modules is not news.
                    if installed_software(&dir, home.as_deref()).is_none()
                        && !matches!(name, "node_modules" | "site-packages" | "dist-packages")
                    {
                        on(Event::Skipped {
                            path,
                            reason: Skip::Installed(what),
                        });
                    }
                    continue;
                }
                if excluded.contains(&path) {
                    on(Event::Skipped {
                        path,
                        reason: Skip::Excluded,
                    });
                    continue;
                }
                let meta = match entry.metadata() {
                    Ok(m) => m,
                    Err(e) => {
                        let reason = Skip::Unreadable(e.to_string());
                        on(Event::Skipped { path, reason });
                        continue;
                    }
                };
                let child = file_id(&meta);
                if child.dev != id.dev {
                    on(Event::Skipped {
                        path,
                        reason: Skip::OtherVolume,
                    });
                    continue;
                }
                if depth >= opts.max_depth {
                    on(Event::Skipped {
                        path,
                        reason: Skip::DepthLimit,
                    });
                    continue;
                }
                if visited.insert((child.dev, child.ino)) {
                    next.push((path, depth + 1));
                }
            }
            stack.extend(next.into_iter().rev());
        }
    }
}

/// Reports `dir` if any rule detects it. Every artifact path of a matching
/// rule is claimed, present or not and with or without evidence, so the walk
/// never enters it and no later project reports it again. Returns false when
/// `dir` belongs to its parent's project (Flutter's `android/` and `ios/`)
/// and must not be walked.
fn report_project(
    rules: &[RuleFile],
    dir: &Path,
    claimed: &mut HashSet<PathBuf>,
    on: &mut impl FnMut(Event),
) -> bool {
    let matched: Vec<&RuleFile> = rules.iter().filter(|r| r.detects(dir)).collect();
    if matched.is_empty() {
        return !rules.iter().any(|r| r.defers_to_parent(dir));
    }
    let mut candidates = Vec::new();
    for rule in &matched {
        for c in rule.candidates(dir) {
            if !claimed.contains(&dir.join(c.rel())) {
                candidates.push(c);
            }
        }
    }
    for rule in &matched {
        claimed.extend(rule.artifacts.iter().map(|a| dir.join(&a.path)));
    }
    let mut seen = HashSet::new();
    candidates.retain(|c| seen.insert(c.rel().to_path_buf()));
    on(Event::Project {
        root: dir.to_path_buf(),
        rules: matched.iter().map(|r| r.id.clone()).collect(),
        candidates,
    });
    true
}
