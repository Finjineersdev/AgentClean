import Foundation

/// Sizes in decimal units (KB, MB, GB), as Finder shows them.
public enum ByteFormatter {
    public static func string(_ bytes: UInt64) -> String {
        let f = ByteCountFormatter()
        f.countStyle = .file
        f.allowsNonnumericFormatting = false  // "0 bytes", not "Zero KB"
        return f.string(fromByteCount: Int64(clamping: bytes))
    }
}
