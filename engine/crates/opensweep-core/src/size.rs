//! Disk usage of a directory tree.
//!
//! Counts allocated space, not file length, because "space freed" is what the
//! user sees. Hard-linked files are counted once, symbolic links are counted
//! as links and never followed, and the walk does not cross onto another
//! volume. Anything unreadable is counted in `unreadable` rather than guessed.
//!
//! Space a tree shares with files outside it cannot be freed by removing the
//! tree. Hard links are seen directly. APFS clones are not visible without
//! private file system calls, so the package managers known to clone or link
//! from a global store (pnpm, Bun) mark their `node_modules` as `shared`.

use std::collections::HashMap;
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
    /// Some of `bytes` may also belong to files outside the tree (a hard link
    /// to elsewhere, or a pnpm or Bun install that clones from a store), so
    /// removing the tree can free less. `bytes` is then an upper estimate.
    pub shared: bool,
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
fn identity(meta: &fs::Metadata) -> (u64, u64, u64) {
    use std::os::unix::fs::MetadataExt;
    (meta.dev(), meta.ino(), meta.nlink())
}

#[cfg(not(unix))]
fn identity(_meta: &fs::Metadata) -> (u64, u64, u64) {
    (0, 0, 1)
}

/// A `node_modules` that pnpm or Bun installed: their files are clones or
/// hard links of a global store, so removing it frees little.
fn from_store(root: &Path) -> bool {
    root.file_name().is_some_and(|n| n == "node_modules")
        && (root.join(".pnpm").is_dir()
            || root.parent().is_some_and(|p| {
                ["bun.lock", "bun.lockb", "pnpm-lock.yaml"]
                    .iter()
                    .any(|f| p.join(f).is_file())
            }))
}

/// Measures `root`. Never follows a symbolic link, including `root` itself.
pub fn usage(root: &Path) -> Usage {
    let mut usage = Usage::default();
    let Ok(root_meta) = fs::symlink_metadata(root) else {
        usage.unreadable += 1;
        return usage;
    };
    let (root_dev, _, _) = identity(&root_meta);
    usage.shared = root_meta.is_dir() && from_store(root);
    // Hard-linked files: links not yet seen inside the tree.
    let mut linked: HashMap<(u64, u64), u64> = HashMap::new();
    let mut stack: Vec<(PathBuf, fs::Metadata)> = vec![(root.to_path_buf(), root_meta)];

    while let Some((path, meta)) = stack.pop() {
        let (dev, ino, links) = identity(&meta);
        if links <= 1 || meta.is_dir() {
            usage.bytes = usage.bytes.saturating_add(allocated(&meta));
        } else if let Some(rest) = linked.get_mut(&(dev, ino)) {
            *rest = rest.saturating_sub(1);
        } else {
            linked.insert((dev, ino), links - 1);
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
    usage.shared |= linked.values().any(|&rest| rest > 0);
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
        assert!(!usage(tmp.path()).shared, "both links are inside");

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
                unreadable: 1,
                shared: false
            }
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_link_to_a_file_outside_is_shared() {
        let tmp = tempfile::tempdir().unwrap();
        let store = tmp.path().join("store");
        let tree = tmp.path().join("tree");
        fs::create_dir_all(&store).unwrap();
        fs::create_dir_all(&tree).unwrap();
        fs::write(store.join("f"), vec![1u8; 50_000]).unwrap();
        assert!(!usage(&tree).shared);
        fs::hard_link(store.join("f"), tree.join("f")).unwrap();
        assert!(usage(&tree).shared);
    }

    #[test]
    fn pnpm_and_bun_installs_are_shared() {
        let tmp = tempfile::tempdir().unwrap();
        let nm = tmp.path().join("node_modules");
        fs::create_dir_all(nm.join("left-pad")).unwrap();
        assert!(!usage(&nm).shared, "npm copies files");
        fs::write(tmp.path().join("bun.lock"), "{}").unwrap();
        assert!(usage(&nm).shared);
        fs::remove_file(tmp.path().join("bun.lock")).unwrap();
        fs::create_dir_all(nm.join(".pnpm")).unwrap();
        assert!(usage(&nm).shared);
    }
}
