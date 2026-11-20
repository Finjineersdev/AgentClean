//! The deletion safety guard.
//!
//! Nothing in OpenSweep may move or delete a path without an [`Approved`]
//! token from [`Guard::check`], and the token must pass [`Approved::reverify`]
//! immediately before the move. The guard never deletes anything itself.
//!
//! The rule is "refuse when unsure": any error while checking is a refusal.
//! See `docs/SAFETY-SPEC.md` for the invariants and the reasoning.

use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

use crate::activity::{Activity, Verdict};
use crate::deny::{protected_component, protected_target};
use crate::rules::{Candidate, Class};

/// Why a path was refused. Each variant maps to an invariant in the spec.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    #[error("the project's .opensweep.toml keeps this folder")]
    KeptByRepository,
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
    #[error("the project is inside installed software ({0}): reinstalling it is not a rebuild")]
    InstalledSoftware(&'static str),
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
    #[error("the project is pinned: it is never cleaned")]
    Pinned,
    #[error("the project is in use: {0}")]
    ActiveProject(String),
}

/// Identity of a directory at approval time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FileId {
    pub(crate) dev: u64,
    pub(crate) ino: u64,
    /// Creation time where the file system records it. Inode numbers are
    /// reused, so a directory deleted and recreated can keep its number.
    born: Option<std::time::SystemTime>,
}

pub(crate) fn file_id(meta: &fs::Metadata) -> FileId {
    // One function with the platform split inside, so the tests that run
    // exercise the code that ships. Windows support lands with the Windows
    // port; until then identity there is the creation time plus the canonical
    // path, which `reverify` also compares.
    #[cfg(unix)]
    let (dev, ino) = {
        use std::os::unix::fs::MetadataExt;
        (meta.dev(), meta.ino())
    };
    #[cfg(not(unix))]
    let (dev, ino) = (0, 0);
    FileId {
        dev,
        ino,
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
        // `symlink_metadata` never reports a link as a directory, so this also
        // refuses a directory that was replaced by a link.
        if !meta.is_dir() || file_id(&meta) != self.id {
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
    allow_active: bool,
}

impl Guard {
    /// `home` is the user's home folder; a project root that is the home
    /// folder or one of its ancestors is refused.
    pub fn new(home: Option<PathBuf>) -> Self {
        let home = home.and_then(|h| fs::canonicalize(h).ok());
        Self {
            home,
            check_git: true,
            allow_active: false,
        }
    }

    /// Also approves folders in projects that are in use. Pinned projects
    /// stay refused.
    pub fn allow_active(mut self) -> Self {
        self.allow_active = true;
        self
    }

    /// Uses `$HOME` for the home folder.
    pub fn from_env() -> Self {
        Self::new(std::env::var_os("HOME").map(PathBuf::from))
    }

    /// Checks one rule candidate in the project `activity` assessed.
    pub fn check(&self, activity: &Activity, candidate: &Candidate) -> Result<Approved, Refusal> {
        if activity.pinned() {
            return Err(Refusal::Pinned);
        }
        // A kept folder protects everything around it: the folder itself,
        // anything inside it, and any artifact that contains it. Case is
        // ignored, as macOS ignores it; refusing more on Linux is safe.
        let folded = |p: &Path| PathBuf::from(p.to_string_lossy().to_lowercase());
        let rel = folded(candidate.rel());
        if activity
            .kept()
            .iter()
            .map(|k| folded(k))
            .any(|k| rel.starts_with(&k) || k.starts_with(&rel))
        {
            return Err(Refusal::KeptByRepository);
        }
        if activity.verdict() == Verdict::Active && !self.allow_active {
            return Err(Refusal::ActiveProject(activity.reason().to_owned()));
        }
        let project_root = activity.root();
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
        if too_shallow(&root) {
            return Err(Refusal::ProjectRootTooBroad);
        }
        if let Some(home) = &self.home {
            if home.starts_with(&root) {
                return Err(Refusal::ProjectRootTooBroad);
            }
        }
        if let Some(what) = installed_software(&root, self.home.as_deref()) {
            return Err(Refusal::InstalledSoftware(what));
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

/// True for `/` and top-level folders such as `/Users`.
/// Folders that only ever hold software someone installed.
const INSTALLED_PREFIXES: &[(&str, &str)] = &[
    ("/opt/homebrew", "Homebrew"),
    ("/usr/local", "Homebrew or a system install"),
    ("/home/linuxbrew", "Homebrew"),
    ("/opt/local", "MacPorts"),
    ("/nix", "Nix"),
    ("/Applications", "an installed app"),
    ("/Library", "the system library"),
    ("/System", "the operating system"),
    ("/usr", "the operating system"),
];

/// The same, relative to the home folder.
const INSTALLED_IN_HOME: &[(&str, &str)] = &[
    (".nvm", "nvm"),
    (".volta", "Volta"),
    (".asdf", "asdf"),
    (".local/share/mise", "mise"),
    (".pyenv", "pyenv"),
    (".rustup", "rustup"),
    (".cargo", "Cargo"),
    (".sdkman", "SDKMAN"),
    (".bun", "Bun"),
    (".deno", "Deno"),
    (".pub-cache", "the Dart package cache"),
    (".gradle", "Gradle"),
    (".m2", "Maven"),
    ("go/pkg", "the Go module cache"),
    ("Library/pnpm", "pnpm"),
    ("Library/Developer", "Xcode"),
    ("Library/Application Support", "an app's data"),
    ("Library/Caches", "a cache"),
    ("Library/Containers", "an app's data"),
    ("Library/Group Containers", "an app's data"),
];

/// Folder names that mean "an installed package lives below here".
const INSTALLED_COMPONENTS: &[(&str, &str)] = &[
    ("node_modules", "an npm package"),
    ("site-packages", "a Python package"),
    ("dist-packages", "a Python package"),
    ("Cellar", "Homebrew"),
];

/// I21: what installed software `root` is part of, if any. Such a "project"
/// is a package someone installed: removing its folders breaks it.
pub fn installed_software(root: &Path, home: Option<&Path>) -> Option<&'static str> {
    for c in root.components() {
        if let Component::Normal(name) = c {
            if let Some((_, what)) = INSTALLED_COMPONENTS.iter().find(|(n, _)| name == *n) {
                return Some(what);
            }
        }
    }
    let prefixes = INSTALLED_PREFIXES
        .iter()
        .map(|(p, w)| (PathBuf::from(p), *w));
    let in_home = home
        .into_iter()
        .flat_map(|h| INSTALLED_IN_HOME.iter().map(move |(p, w)| (h.join(p), *w)));
    prefixes
        .chain(in_home)
        .find(|(p, _)| root.starts_with(p))
        .map(|(_, w)| w)
}

fn too_shallow(root: &Path) -> bool {
    let depth = root
        .components()
        .filter(|c| matches!(c, Component::Normal(_)))
        .count();
    depth < MIN_ROOT_DEPTH
}

/// git in `root`, with every setting that could run a program supplied by
/// the (untrusted) repository switched off. Callers add the subcommand.
/// The folders of a PATH value that are absolute. An empty or relative
/// entry means "the current folder", which inside a scanned repository would
/// find the repository's own `git` or `npm`.
pub(crate) fn absolute_dirs(path: Option<&std::ffi::OsStr>) -> Vec<PathBuf> {
    path.map(|p| {
        std::env::split_paths(p)
            .filter(|d| d.is_absolute())
            .collect()
    })
    .unwrap_or_default()
}

/// git, found through absolute PATH folders only.
fn git_program(path: Option<&std::ffi::OsStr>) -> PathBuf {
    absolute_dirs(path)
        .into_iter()
        .map(|d| d.join("git"))
        .find(|g| g.is_file())
        .unwrap_or_else(|| PathBuf::from("/usr/bin/git"))
}

pub(crate) fn hardened_git(root: &Path) -> Command {
    let mut git = Command::new(git_program(std::env::var_os("PATH").as_deref()));
    git.args([
        "-c",
        "core.fsmonitor=false",
        "-c",
        "core.hooksPath=/dev/null",
    ])
    .args([
        "-c",
        "log.showSignature=false",
        "-c",
        "core.pager=cat",
        "-C",
    ])
    .arg(root)
    .env("GIT_OPTIONAL_LOCKS", "0")
    .env("GIT_TERMINAL_PROMPT", "0")
    // A partial clone fetches missing objects from a remote the repository
    // names, over a transport it chooses (`ext::` runs a command). No lazy
    // fetches, and no transport at all: an empty GIT_ALLOW_PROTOCOL overrides
    // even a repository's own `protocol.<name>.allow`.
    .env("GIT_NO_LAZY_FETCH", "1")
    .env("GIT_ALLOW_PROTOCOL", "")
    .env_remove("GIT_DIR")
    .env_remove("GIT_WORK_TREE")
    .stdin(Stdio::null())
    .stderr(Stdio::null());
    git
}

/// True if `dir` or any ancestor holds a `.git` entry.
pub(crate) fn inside_git_checkout(dir: &Path) -> bool {
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
    let output = hardened_git(root)
        .args([
            OsStr::new("ls-files"),
            OsStr::new("-z"),
            OsStr::new("--"),
            rel.as_os_str(),
        ])
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

#[cfg(test)]
mod tests {
    use super::too_shallow;
    use std::path::Path;

    #[test]
    fn roots_need_two_components() {
        assert!(too_shallow(Path::new("/")));
        assert!(too_shallow(Path::new("/Users")));
        assert!(!too_shallow(Path::new("/Users/dev")));
        assert!(!too_shallow(Path::new("/Users/dev/proj")));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod path_tests {
    use super::{absolute_dirs, git_program};
    use std::ffi::OsStr;

    /// A repository's own `git` is never found through `.` or an empty entry.
    #[cfg(unix)]
    #[test]
    fn relative_path_entries_are_ignored() {
        let dirs = absolute_dirs(Some(OsStr::new(".::bin:/usr/bin")));
        assert_eq!(dirs, vec![std::path::PathBuf::from("/usr/bin")]);

        assert_eq!(
            git_program(Some(OsStr::new(".:"))),
            std::path::PathBuf::from("/usr/bin/git")
        );
    }
}
