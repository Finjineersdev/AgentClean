//! Disk usage of a directory tree.
//!
//! Counts allocated space, not file length, because "space freed" is what the
//! user sees. Hard-linked files are counted once, symbolic links are counted
//! as links and never followed, and the walk does not cross onto another
//! volume. Anything unreadable is counted in `unreadable` rather than guessed.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    /// Allocated bytes.
    pub bytes: u64,
    /// Entries counted, directories included.
    pub entries: u64,
    /// Entries that could not be read; `bytes` is a lower bound when non-zero.
    pub unreadable: u64,
}

#[cfg(unix)]
fn allocated(meta: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    meta.blocks().saturating_mul(512)
}

#[cfg(not(unix))]
fn allocated(meta: &fs::Metadata) -> u64 {
    meta.len()
}

#[cfg(unix)]
fn identity(meta: &fs::Metadata) -> (u64, u64, bool) {
    use std::os::unix::fs::MetadataExt;
    (meta.dev(), meta.ino(), meta.nlink() > 1)
}

#[cfg(not(unix))]
fn identity(_meta: &fs::Metadata) -> (u64, u64, bool) {
    (0, 0, false)
}

/// Measures `root`. Never follows a symbolic link, including `root` itself.
pub fn usage(root: &Path) -> Usage {
    let mut usage = Usage::default();
    let Ok(root_meta) = fs::symlink_metadata(root) else {
        usage.unreadable += 1;
        return usage;
    };
    let (root_dev, _, _) = identity(&root_meta);
    let mut linked: HashSet<(u64, u64)> = HashSet::new();
    let mut stack: Vec<(PathBuf, fs::Metadata)> = vec![(root.to_path_buf(), root_meta)];

    while let Some((path, meta)) = stack.pop() {
        let (dev, ino, shared) = identity(&meta);
        if !shared || linked.insert((dev, ino)) {
            usage.bytes = usage.bytes.saturating_add(allocated(&meta));
        }
        usage.entries += 1;
        if !meta.is_dir() || meta.file_type().is_symlink() || dev != root_dev {
            continue;
        }
        let Ok(children) = fs::read_dir(&path) else {
            usage.unreadable += 1;
            continue;
        };
        for child in children {
            match child.and_then(|c| {
                let p = c.path();
                fs::symlink_metadata(&p).map(|m| (p, m))
            }) {
                Ok(pair) => stack.push(pair),
                Err(_) => usage.unreadable += 1,
            }
        }
    }
    usage
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::usage;
    use std::fs;

    #[test]
    fn counts_files_and_directories() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("a/b")).unwrap();
        fs::write(tmp.path().join("a/b/f"), vec![7u8; 20_000]).unwrap();
        let u = usage(tmp.path());
        assert_eq!(u.entries, 4);
        assert_eq!(u.unreadable, 0);
        assert!(u.bytes >= 20_000, "{u:?}");
    }

    #[cfg(unix)]
    #[test]
    fn hard_links_count_once_and_symlinks_are_not_followed() {
        let tmp = tempfile::tempdir().unwrap();
        let big = tmp.path().join("big");
        fs::write(&big, vec![1u8; 200_000]).unwrap();
        let one = usage(tmp.path()).bytes;

        fs::hard_link(&big, tmp.path().join("big2")).unwrap();
        assert_eq!(usage(tmp.path()).bytes, one);

        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("huge"), vec![1u8; 400_000]).unwrap();
        std::os::unix::fs::symlink(outside.path(), tmp.path().join("link")).unwrap();
        assert!(usage(tmp.path()).bytes < one + 100_000);
    }

    #[test]
    fn missing_path_is_reported_not_guessed() {
        let u = usage(std::path::Path::new("/definitely/not/here"));
        assert_eq!(
            u,
            super::Usage {
                bytes: 0,
                entries: 0,
                unreadable: 1
            }
        );
    }
}
