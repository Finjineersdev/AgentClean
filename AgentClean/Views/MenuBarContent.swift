import SwiftUI
import AppKit
import AgentEngine

/// The menu bar menu: the last scan's total and a way back to the window.
/// It never cleans; cleaning always goes through the review in the window.
struct MenuBarContent: View {
    @Environment(AppModel.self) private var model
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        if model.scanning {
            Text("Scanning… \(model.projects.count) projects")
        } else if let s = model.summary {
            Text("\(bytes(s.ready.bytes)) ready to clean")
            Text("\(s.projects) projects · \(bytes(s.caution.bytes)) Caution")
        } else {
            Text("Not scanned yet")
        }
        Divider()
        Button(model.scanning ? "Stop Scan" : "Scan Now") { model.toggleScan() }
        Button("Open AgentClean") { open(.overview) }
            .keyboardShortcut("o")
        Button("Review Projects…") { open(.projects) }
            .disabled(model.summary == nil)
        Divider()
        if Updater.controller != nil {
            Button("Check for Updates…") { Updater.checkForUpdates() }
        }
        SettingsLink { Text("Settings…") }
            .keyboardShortcut(",")
        Button("Quit AgentClean") { NSApp.terminate(nil) }
            .keyboardShortcut("q")
    }

    private func open(_ screen: AppModel.Screen) {
        model.screen = screen
        openWindow(id: "main")
        NSApp.activate(ignoringOtherApps: true)
    }
}
