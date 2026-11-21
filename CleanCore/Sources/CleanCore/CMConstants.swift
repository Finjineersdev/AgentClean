import Foundation

/// Well-known paths. Nothing in here is user-configurable.
///
/// Path tables adapted from Mac Sai (BSD-3-Clause, © iliyami contributors).
public enum CMConstants {
    public static let appName = "OpenSweep"

    /// Home path without the trailing slash URL.path adds, for string prefix work.
    public static let homePath: String = {
        var p = FileManager.default.homeDirectoryForCurrentUser.path(percentEncoded: false)
        while p.count > 1 && p.hasSuffix("/") { p.removeLast() }
        return p
    }()

    /// ~/Library/Application Support/OpenSweep: the app's own small caches.
    public static let appSupportDir = FileManager.default.homeDirectoryForCurrentUser
        .appending(path: "Library/Application Support").appending(path: appName)
}
