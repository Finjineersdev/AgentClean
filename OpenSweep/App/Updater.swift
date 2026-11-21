import Foundation
import Sparkle

/// Sparkle updates, the app's only network use (CLAUDE.md rule 5). Off unless
/// the build carries a public key (release builds); Sparkle asks the user
/// before its first automatic check.
@MainActor
enum Updater {
    static let controller: SPUStandardUpdaterController? = {
        let key = Bundle.main.object(forInfoDictionaryKey: "SUPublicEDKey") as? String ?? ""
        guard !key.isEmpty, !key.hasPrefix("$(") else { return nil }
        return SPUStandardUpdaterController(startingUpdater: true, updaterDelegate: nil, userDriverDelegate: nil)
    }()

    static func checkForUpdates() { controller?.checkForUpdates(nil) }

    /// Whether Sparkle checks on its own; nil when this build has no updater.
    static var automaticChecks: Bool? {
        get { controller?.updater.automaticallyChecksForUpdates }
        set { if let newValue { controller?.updater.automaticallyChecksForUpdates = newValue } }
    }
}
