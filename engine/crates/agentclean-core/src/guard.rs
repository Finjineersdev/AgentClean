//! The deletion safety guard.
//!
//! Nothing in AgentClean may move or delete a path without an [`Approved`]
//! token from [`Guard::check`], and the token must pass [`Approved::reverify`]
//! immediately before the move. The guard never deletes anything itself.
//!
//! The rule is "refuse when unsure": any error while checking is a refusal.
//! See `docs/SAFETY-SPEC.md` for the invariants and the reasoning.

use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

use crate::deny::{protected_component, protected_target};
use crate::rules::{Candidate, Class};

/// Why a path was refused. Each variant maps to an invariant in the spec.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    #[error("path is not absolute")]
    NotAbsolute,
    #[error("path contains '.' or '..'")]
    NotNormalized,
    #[error("path does not exist or cannot be read")]
    NotFound,
    #[error("path is a symbolic link")]
    Symlink,
    #[error("path is not a directory")]
    NotADirectory,
    #[error("path is outside the project root")]
    OutsideProject,
    #[error("path is the project root itself")]
    IsProjectRoot,
    #[error("a symbolic link sits between the project root and the path")]
    SymlinkInPath,
    #[error("project root is too close to the top of the disk or contains the home folder")]
    ProjectRootTooBroad,
    #[error("component {0:?} is protected: {1}")]
    Protected(String, &'static str),
    #[error("path is on a different volume from the project root")]
    CrossDevice,
    #[error("path contains its own repository")]
    ContainsRepository,
    #[error("path contains files tracked by git")]
    ContainsTrackedFiles,
    #[error("the project is a git repository but tracked files could not be checked")]
    VcsCheckFailed,
    #[error("path changed since it was approved")]
    ChangedSinceApproval,
}

/// Identity of a directory at approval time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileId {
    dev: u64,
    ino: u64,
    /// Creation time where the file system records it. Inode numbers are
    /// reused, so a directory deleted and recreated can keep its number.
    born: Option<std::time::SystemTime>,
}

#[cfg(unix)]
fn file_id(meta: &fs::Metadata) -> FileId {
    use std::os::unix::fs::MetadataExt;
    FileId {
        dev: meta.dev(),
        ino: meta.ino(),
        born: meta.created().ok(),
    }
}

#[cfg(not(unix))]
fn file_id(meta: &fs::Metadata) -> FileId {
    // Windows support lands with the Windows port; until then identity is the
    // creation time plus the canonical path, which `reverify` also compares.
    FileId {
        dev: 0,
        ino: 0,
        born: meta.created().ok(),
    }
}

/// Proof that a path passed every check. Cannot be constructed outside this
/// module, so holding one means [`Guard::check`] returned `Ok`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Approved {
    path: PathBuf,
    project_root: PathBuf,
    id: FileId,
    rule_id: String,
    class: Class,
}

impl Approved {
    /// Canonical path of the approved directory.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Canonical project root the path was approved under.
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// Rule that produced the candidate.
    pub fn rule_id(&self) -> &str {
        &self.rule_id
    }

    /// Class the rule gave the candidate.
    pub fn class(&self) -> Class {
        self.class
    }

    /// Device the directory was on when approved.
    pub(crate) fn device(&self) -> u64 {
        self.id.dev
    }

    /// Must be called immediately before moving the path. Refuses if the
    /// directory was replaced, turned into a link, or moved since approval.
    pub fn reverify(&self) -> Result<(), Refusal> {
        let meta = fs::symlink_metadata(&self.path).map_err(|_| Refusal::ChangedSinceApproval)?;
        if meta.file_type().is_symlink() || !meta.is_dir() || file_id(&meta) != self.id {
            return Err(Refusal::ChangedSinceApproval);
        }
        match fs::canonicalize(&self.path) {
            Ok(now) if now == self.path => Ok(()),
            _ => Err(Refusal::ChangedSinceApproval),
        }
    }
}

/// Smallest number of components a project root may have (`/a/b`).
const MIN_ROOT_DEPTH: usize = 2;

#[derive(Debug, Clone)]
pub struct Guard {
    home: Option<PathBuf>,
    check_git: bool,
}

impl Guard {
    /// `home` is the user's home folder; a project root that is the home
    /// folder or one of its ancestors is refused.
    pub fn new(home: Option<PathBuf>) -> Self {
        let home = home.and_then(|h| fs::canonicalize(h).ok());
        Self {
            home,
            check_git: true,
        }
    }

    /// Uses `$HOME` for the home folder.
    pub fn from_env() -> Self {
        Self::new(std::env::var_os("HOME").map(PathBuf::from))
    }

    /// Checks one rule candidate under one project root.
    pub fn check(&self, project_root: &Path, candidate: &Candidate) -> Result<Approved, Refusal> {
        if !project_root.is_absolute() {
            return Err(Refusal::NotAbsolute);
        }
        let lexical_rel = candidate.rel();
        let rel_is_plain = !lexical_rel.as_os_str().is_empty()
            && lexical_rel
                .components()
                .all(|c| matches!(c, Component::Normal(_)));
        if !is_normalized(project_root) || !rel_is_plain {
            return Err(Refusal::NotNormalized);
        }
        let target = &project_root.join(lexical_rel);

        let root = fs::canonicalize(project_root).map_err(|_| Refusal::NotFound)?;
        let root_meta = fs::symlink_metadata(&root).map_err(|_| Refusal::NotFound)?;
        if !root_meta.is_dir() {
            return Err(Refusal::NotADirectory);
        }
        if depth(&root) < MIN_ROOT_DEPTH {
            return Err(Refusal::ProjectRootTooBroad);
        }
        if let Some(home) = &self.home {
            if home.starts_with(&root) {
                return Err(Refusal::ProjectRootTooBroad);
            }
        }

        let meta = fs::symlink_metadata(target).map_err(|_| Refusal::NotFound)?;
        if meta.file_type().is_symlink() {
            return Err(Refusal::Symlink);
        }
        if !meta.is_dir() {
            return Err(Refusal::NotADirectory);
        }

        let canon = fs::canonicalize(target).map_err(|_| Refusal::NotFound)?;
        let rel = canon
            .strip_prefix(&root)
            .map_err(|_| Refusal::OutsideProject)?
            .to_path_buf();
        if rel.as_os_str().is_empty() {
            return Err(Refusal::IsProjectRoot);
        }

        // The path as given, below the root, must be the path on disk: no
        // symlinked component may redirect it, even to somewhere in the project.
        if root.join(lexical_rel) != canon {
            return Err(Refusal::SymlinkInPath);
        }

        let last = rel.file_name();
        for component in rel.components() {
            let Component::Normal(os) = component else {
                return Err(Refusal::NotNormalized);
            };
            let name = os.to_str().ok_or(Refusal::NotNormalized)?;
            let verdict = if Some(os) == last {
                protected_target(name)
            } else {
                protected_component(name)
            };
            if let Some(why) = verdict {
                return Err(Refusal::Protected(name.to_owned(), why));
            }
        }

        let id = file_id(&meta);
        if id.dev != file_id(&root_meta).dev {
            return Err(Refusal::CrossDevice);
        }

        if fs::symlink_metadata(canon.join(".git")).is_ok() {
            return Err(Refusal::ContainsRepository);
        }

        if self.check_git {
            check_untracked(&root, &rel)?;
        }

        Ok(Approved {
            path: canon,
            project_root: root,
            id,
            rule_id: candidate.rule_id().to_owned(),
            class: candidate.class(),
        })
    }
}

fn is_normalized(path: &Path) -> bool {
    path.components().all(|c| {
        matches!(
            c,
            Component::RootDir | Component::Prefix(_) | Component::Normal(_)
        )
    })
}

fn depth(path: &Path) -> usize {
    path.components()
        .filter(|c| matches!(c, Component::Normal(_)))
        .count()
}

/// True if `dir` or any ancestor holds a `.git` entry.
fn inside_git_checkout(dir: &Path) -> bool {
    dir.ancestors()
        .any(|a| fs::symlink_metadata(a.join(".git")).is_ok())
}

/// Refuses if git tracks anything under `rel`.
///
/// The repository being inspected is untrusted input, so git runs with the
/// settings that could execute repository-supplied programs switched off.
fn check_untracked(root: &Path, rel: &Path) -> Result<(), Refusal> {
    if !inside_git_checkout(root) {
        return Ok(());
    }
    let output = Command::new("git")
        .args([
            OsStr::new("-c"),
            OsStr::new("core.fsmonitor=false"),
            OsStr::new("-c"),
            OsStr::new("core.hooksPath=/dev/null"),
            OsStr::new("-C"),
            root.as_os_str(),
            OsStr::new("ls-files"),
            OsStr::new("-z"),
            OsStr::new("--"),
            rel.as_os_str(),
        ])
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|_| Refusal::VcsCheckFailed)?;
    if !output.status.success() {
        return Err(Refusal::VcsCheckFailed);
    }
    if output.stdout.is_empty() {
        Ok(())
    } else {
        Err(Refusal::ContainsTrackedFiles)
    }
}
