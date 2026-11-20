import SwiftUI

@main
struct AgentCleanApp: App {
    @State private var model = AppModel()
    @AppStorage("showMenuBar") private var showMenuBar = true

    var body: some Scene {
        WindowGroup(id: "main") {
            RootView()
                .environment(model)
                .frame(minWidth: 980, minHeight: 640)
        }
        .windowStyle(.hiddenTitleBar)
        .defaultSize(width: 1180, height: 760)
        .commands {
            if Updater.controller != nil {
                CommandGroup(after: .appInfo) {
                    Button("Check for Updates…") { Updater.checkForUpdates() }
                }
            }
        }

        MenuBarExtra("AgentClean", systemImage: "sparkles", isInserted: $showMenuBar) {
            MenuBarContent().environment(model)
        }

        Settings {
            SettingsView().environment(model).frame(width: 600, height: 600)
        }
    }
}
