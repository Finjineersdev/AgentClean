//! Drives other than the one holding the main quarantine (ADR 0004).
//!
//! A clean only renames, and a rename cannot cross drives, so each drive gets
//! its own quarantine at `<drive>/.agentclean-quarantine`. Network drives are
//! never used: a rename there may not be atomic and ownership may be faked.

use std::path::{Path, PathBuf};

/// A mounted file system, from the system's mount table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    pub path: PathBuf,
    pub fs_type: String,
    pub network: bool,
}

/// File system types that are network or remote mounts.
const NETWORK_TYPES: &[&str] = &[
    "nfs",
    "nfs4",
    "smbfs",
    "smb3",
    "cifs",
    "afpfs",
    "webdav",
    "davfs",
    "ftp",
    "sshfs",
    "fuse.sshfs",
    "9p",
    "afs",
    "ceph",
    "glusterfs",
    "fuse.rclone",
];

/// The current mount table, or None if it cannot be read.
pub fn mounts() -> Option<Vec<Mount>> {
    #[cfg(target_os = "linux")]
    let table = std::fs::read_to_string("/proc/self/mounts")
        .ok()
        .map(|t| parse_proc_mounts(&t));
    #[cfg(not(target_os = "linux"))]
    let table = {
        // A fixed path and no arguments: no shell, no PATH lookup.
        let out = std::process::Command::new("/sbin/mount")
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()
            .filter(|o| o.status.success())?;
        Some(parse_bsd_mount(&String::from_utf8_lossy(&out.stdout)))
    };
    table.filter(|t| !t.is_empty())
}

/// `/proc/self/mounts`: `device path type options 0 0`, with spaces in the
/// path written as `\040`.
pub fn parse_proc_mounts(text: &str) -> Vec<Mount> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let _device = fields.next()?;
            let path = fields.next()?.replace("\\040", " ").replace("\\011", "\t");
            let fs_type = fields.next()?.to_owned();
            let network = NETWORK_TYPES.contains(&fs_type.as_str());
            Some(Mount {
                path: PathBuf::from(path),
                fs_type,
                network,
            })
        })
        .collect()
}

/// macOS `mount`: `device on /path (type, option, ...)`. A mount without the
/// `local` option is a network mount.
pub fn parse_bsd_mount(text: &str) -> Vec<Mount> {
    text.lines()
        .filter_map(|line| {
            let (_, rest) = line.split_once(" on ")?;
            let (path, opts) = rest.rsplit_once(" (")?;
            let opts: Vec<&str> = opts.trim_end_matches(')').split(", ").collect();
            let fs_type = (*opts.first()?).to_owned();
            let network = NETWORK_TYPES.contains(&fs_type.as_str()) || !opts.contains(&"local");
            Some(Mount {
                path: PathBuf::from(path),
                fs_type,
                network,
            })
        })
        .collect()
}

/// The top folder of the drive holding `path`: its highest ancestor on the
/// same device. None if `path` cannot be read.
pub fn drive_root(path: &Path) -> Option<PathBuf> {
    let path = std::fs::canonicalize(path).ok()?;
    let dev = device(&path)?;
    let mut top = path.clone();
    for ancestor in path.ancestors().skip(1) {
        if device(ancestor) != Some(dev) {
            break;
        }
        top = ancestor.to_path_buf();
    }
    Some(top)
}

/// Why the drive at `root` may not hold a quarantine, if it may not.
pub fn unusable(root: &Path, mounts: Option<&[Mount]>) -> Option<String> {
    let Some(mounts) = mounts else {
        return Some("the list of drives could not be read".to_owned());
    };
    match mounts.iter().rev().find(|m| m.path == root) {
        None => Some("the drive's type could not be determined".to_owned()),
        Some(m) if m.network => Some(format!("on a network drive ({})", m.fs_type)),
        Some(_) => None,
    }
}

pub(crate) fn device(path: &Path) -> Option<u64> {
    let meta = std::fs::metadata(path).ok()?;
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
    Some(dev)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn linux_mount_table_is_parsed() {
        let text = "\
/dev/sda1 / ext4 rw,relatime 0 0
tmpfs /dev/shm tmpfs rw,nosuid 0 0
//nas/share /mnt/my\\040share cifs rw 0 0
server:/x /mnt/nfs nfs4 rw 0 0
short line
";
        let m = parse_proc_mounts(text);
        assert_eq!(m.len(), 4);
        assert_eq!(m[1].path, PathBuf::from("/dev/shm"));
        assert_eq!((m[1].fs_type.as_str(), m[1].network), ("tmpfs", false));
        assert_eq!(m[2].path, PathBuf::from("/mnt/my share"));
        assert!(m[2].network && m[3].network && !m[0].network);
    }

    #[test]
    fn macos_mount_table_is_parsed() {
        let text = "\
/dev/disk3s5 on /System/Volumes/Data (apfs, local, journaled, nobrowse)
/dev/disk9s1 on /Volumes/My Drive (on SSD) (exfat, local, nodev, nosuid, noowners)
//me@nas/share on /Volumes/share (smbfs, nodev, nosuid, mounted by me)
map auto_home on /System/Volumes/Data/home (autofs, automounted, nobrowse)
garbage
";
        let m = parse_bsd_mount(text);
        assert_eq!(m.len(), 4);
        assert_eq!(m[1].path, PathBuf::from("/Volumes/My Drive (on SSD)"));
        assert_eq!((m[1].fs_type.as_str(), m[1].network), ("exfat", false));
        assert_eq!((m[2].fs_type.as_str(), m[2].network), ("smbfs", true));
        // Not marked local: treated as remote.
        assert!(m[3].network && !m[0].network);
    }

    #[test]
    fn only_known_local_drives_are_usable() {
        let mounts = parse_bsd_mount(
            "/dev/disk9s1 on /Volumes/USB (exfat, local)\n//x@nas/s on /Volumes/nas (smbfs, nodev)\n",
        );
        assert_eq!(unusable(Path::new("/Volumes/USB"), Some(&mounts)), None);
        assert_eq!(
            unusable(Path::new("/Volumes/nas"), Some(&mounts)).as_deref(),
            Some("on a network drive (smbfs)")
        );
        assert!(unusable(Path::new("/Volumes/other"), Some(&mounts)).is_some());
        assert!(unusable(Path::new("/Volumes/USB"), None).is_some());
    }

    #[test]
    fn a_drive_root_is_the_highest_folder_on_the_same_device() {
        let tmp = tempfile::tempdir().unwrap();
        let deep = tmp.path().join("a/b");
        std::fs::create_dir_all(&deep).unwrap();
        let root = drive_root(&deep).unwrap();
        let canon = std::fs::canonicalize(&deep).unwrap();
        assert!(canon.starts_with(&root));
        assert_eq!(device(&root), device(&canon));
        if let Some(up) = root.parent() {
            assert_ne!(device(up), device(&root));
        }
        assert_eq!(drive_root(&tmp.path().join("missing")), None);
    }
}
