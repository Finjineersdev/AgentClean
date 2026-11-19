import SwiftUI

@main
struct AgentCleanApp: App {
    @State private var model = AppModel()

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(model)
                .frame(minWidth: 980, minHeight: 640)
                .preferredColorScheme(.dark)
        }
        .windowStyle(.hiddenTitleBar)
        .defaultSize(width: 1180, height: 760)

        Settings {
            SettingsView().environment(model).frame(width: 600, height: 560)
        }
    }
}
