import Foundation

/// Fast recursive directory sizing on BSD APIs, by the engine's rules
/// (`size.rs`): hard links count once, links are not followed, and the walk
/// stays on the volume it starts on.
public enum DirectorySizer {
    public struct Sizes: Sendable {
        public let allocated: UInt64
        public let fileCount: Int
    }

    public static func size(of url: URL) -> Sizes {
        var allocated: UInt64 = 0
        var count = 0
        var seen = Set<FileID>()
        let path = url.path(percentEncoded: false)
        var root = stat()
        let device = lstat(path, &root) == 0 ? root.st_dev : -1
        walk(path, device, &allocated, &count, &seen)
        return Sizes(allocated: allocated, fileCount: count)
    }

    /// A file's identity: inode numbers are unique only within a volume.
    private struct FileID: Hashable { let device: dev_t; let inode: ino_t }

    private static func walk(_ path: String, _ device: dev_t, _ allocated: inout UInt64,
                             _ count: inout Int, _ seen: inout Set<FileID>) {
        guard let dir = opendir(path) else {
            // Might be a file, not a directory.
            var st = stat()
            if lstat(path, &st) == 0, (st.st_mode & S_IFMT) == S_IFREG {
                allocated += UInt64(st.st_blocks) * 512
                count += 1
            }
            return
        }
        defer { closedir(dir) }
        let fd = dirfd(dir)
        while let entry = readdir(dir) {
            var nameBuf = entry.pointee.d_name
            let name = withUnsafePointer(to: &nameBuf) {
                $0.withMemoryRebound(to: CChar.self, capacity: Int(MAXPATHLEN)) { String(cString: $0) }
            }
            if name == "." || name == ".." { continue }
            var st = stat()
            let ok = withUnsafePointer(to: &nameBuf) {
                $0.withMemoryRebound(to: CChar.self, capacity: Int(MAXPATHLEN)) {
                    fstatat(fd, $0, &st, AT_SYMLINK_NOFOLLOW) == 0
                }
            }
            guard ok else { continue }
            let mode = st.st_mode & S_IFMT
            if mode == S_IFDIR {
                if st.st_dev == device { walk(path + "/" + name, device, &allocated, &count, &seen) }
            } else if mode == S_IFREG {
                if st.st_nlink > 1, !seen.insert(FileID(device: st.st_dev, inode: st.st_ino)).inserted { continue }
                allocated += UInt64(st.st_blocks) * 512
                count += 1
            }
        }
    }
}
