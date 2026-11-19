import Foundation

/// Fast recursive directory sizing on BSD APIs. Dedupes hard links by inode.
public enum DirectorySizer {
    public struct Sizes: Sendable {
        public let logical: UInt64
        public let allocated: UInt64
        public let fileCount: Int
    }

    public static func size(of url: URL) -> Sizes {
        var logical: UInt64 = 0
        var allocated: UInt64 = 0
        var count = 0
        var seen = Set<UInt64>()
        walk(url.path(percentEncoded: false), &logical, &allocated, &count, &seen)
        return Sizes(logical: logical, allocated: allocated, fileCount: count)
    }

    private static func walk(_ path: String, _ logical: inout UInt64, _ allocated: inout UInt64,
                             _ count: inout Int, _ seen: inout Set<UInt64>) {
        guard let dir = opendir(path) else {
            // Might be a file, not a directory.
            var st = stat()
            if lstat(path, &st) == 0, (st.st_mode & S_IFMT) == S_IFREG {
                logical += UInt64(st.st_size)
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
                walk(path + "/" + name, &logical, &allocated, &count, &seen)
            } else if mode == S_IFREG {
                if st.st_nlink > 1, !seen.insert(UInt64(st.st_ino)).inserted { continue }
                logical += UInt64(st.st_size)
                allocated += UInt64(st.st_blocks) * 512
                count += 1
                ScanTelemetry.shared.visited(count & 63 == 0 ? path + "/" + name : nil, bytes: UInt64(st.st_blocks) * 512)
            }
        }
    }
}
