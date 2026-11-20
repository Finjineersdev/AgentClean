import SwiftUI
import AppKit
import AgentEngine

struct SettingsView: View {
    @Environment(AppModel.self) private var model
    @State private var idle = 14
    @State private var dormant = 90
    @AppStorage("showMenuBar") private var showMenuBar = true
    @AppStorage(Peep.showKey) private var showCharacter = true
    @State private var autoUpdate = Updater.automaticChecks

    var body: some View {
        Form {
            Section {
                Toggle("Show OpenSweep in the menu bar", isOn: $showMenuBar)
                    .accessibilityIdentifier("settings.menuBar")
                Toggle("Show the character and cleaning stats", isOn: $showCharacter)
                    .accessibilityIdentifier("settings.character")
                if let autoUpdate {
                    Toggle("Check for updates automatically", isOn: Binding(get: { autoUpdate }, set: {
                        Updater.automaticChecks = $0
                        self.autoUpdate = $0
                    }))
                    .accessibilityIdentifier("settings.autoUpdate")
                }
            } footer: {
                Text(autoUpdate == nil
                     ? "This build has no update feed, so OpenSweep never goes online."
                     : "The update check is the only time OpenSweep goes online. Stats are counted on this Mac from History.")
                    .foregroundStyle(Brand.muted)
            }
            Section {
                @Bindable var model = model
                Toggle("Scan the whole Mac", isOn: $model.wholeDisk)
                    .accessibilityIdentifier("settings.wholeDisk")
                if model.wholeDisk {
                    Toggle("Also scan other drives", isOn: $model.otherDrives)
                        .accessibilityIdentifier("settings.otherDrives")
                } else {
                    ForEach(model.scanRoots, id: \.self) { root in
                        HStack {
                            Text(tildePath(root))
                            Spacer()
                            Button("Remove") { model.scanRoots.removeAll { $0 == root } }
                                .disabled(model.scanRoots.count == 1)
                        }
                    }
                    Button("Add Folder…") { if let url = pickFolder() { model.scanRoots.append(url.path) } }
                }
                LabeledContent("Full Disk Access") {
                    if model.fullDiskAccess {
                        Label("On", systemImage: "checkmark.circle.fill").foregroundStyle(.green)
                    } else {
                        Button("Open Privacy Settings…") { FullDiskAccess.openSettings() }
                    }
                }
            } header: {
                Text("What to scan")
            } footer: {
                Text(model.wholeDisk
                     ? "Every folder on this Mac's own disk: your home folder, /Applications, /Library, /opt, /usr/local and /private/tmp. Installed software is noted, never cleaned. Other drives are local drives you plugged in; network drives, disk images and Time Machine backups are never scanned."
                     : "Only these folders and everything inside them.")
                    .foregroundStyle(Brand.muted)
            }

            Section {
                @Bindable var model = model
                Picker("Free quarantined space", selection: $model.autoFreeDays) {
                    Text("After 1 day").tag(1)
                    Text("After 7 days").tag(7)
                    Text("After 30 days").tag(30)
                    Text("Only when I purge").tag(0)
                }
                .accessibilityIdentifier("settings.autoFree")
                Toggle("Rescan when OpenSweep opens", isOn: $model.scanOnOpen)
                    .accessibilityIdentifier("settings.scanOnOpen")
                Picker("Clean automatically", selection: $model.autoClean) {
                    Text("Never").tag(AutoClean.off)
                    Text("Every day").tag(AutoClean.daily)
                    Text("Every week").tag(AutoClean.weekly)
                }
                .accessibilityIdentifier("settings.autoClean")
                if model.autoClean != .off {
                    Picker("At most", selection: $model.autoCleanCapGB) {
                        ForEach([5, 10, 20, 50], id: \.self) { Text("\($0) GB").tag($0) }
                    }
                    Toggle("Open OpenSweep at login", isOn: Binding(get: { Scheduler.opensAtLogin }, set: { Scheduler.opensAtLogin = $0 }))
                }
            } header: {
                Text("Space and scans")
            } footer: {
                Text("Cleaned folders wait in quarantine so you can undo; after this long they are deleted for good and the space comes back. The last scan is kept, and refreshed in the background when it is over an hour old. A scheduled clean takes only Safe folders in dormant projects, never pinned ones, and tells you with an Undo button. It runs while OpenSweep is open, including in the menu bar.")
                    .foregroundStyle(Brand.muted)
            }

            Section {
                Stepper("Active if changed within \(idle) days", value: $idle, in: 1...dormant)
                Stepper("Dormant after \(dormant) days", value: $dormant, in: idle...3650)
                Button("Save") { Task { await model.setDays(idle: idle, dormant: dormant) } }
                    .disabled(idle == model.config?.idleDays && dormant == model.config?.dormantDays)
            } header: {
                Text("Activity")
            } footer: {
                Text("Active projects are only cleaned when you say so in the project.").foregroundStyle(Brand.muted)
            }

            Section("Never scan") {
                ForEach(model.config?.exclude ?? [], id: \.self) { folder in
                    HStack {
                        Text(tildePath(folder))
                        Spacer()
                        Button("Remove") { Task { await model.include(folder) } }
                    }
                }
                Button("Add Folder…") { if let url = pickFolder() { Task { await model.exclude(url) } } }
            }

            Section {
                LabeledContent("Your own rules") {
                    Button("Show Folder") {
                        if let path = model.config?.path {
                            let dir = URL(fileURLWithPath: path).deletingLastPathComponent()
                            NSWorkspace.shared.activateFileViewerSelecting([dir.appendingPathComponent("rules")])
                        }
                    }
                }
            } header: {
                Text("Rules")
            } footer: {
                Text("Add TOML rule files to a folder named rules beside config.toml for build output the catalogue does not know. They are checked like the built-in rules. A repository can also carry .opensweep.toml to pin itself or keep folders.")
                    .foregroundStyle(Brand.muted)
            }

            Section("Pinned: never cleaned") {
                if (model.config?.pinned.isEmpty ?? true) && (model.config?.pinnedRemotes ?? []).isEmpty {
                    Text("Nothing is pinned. Pin a project from its page.").foregroundStyle(Brand.muted)
                }
                ForEach(model.config?.pinnedRemotes ?? [], id: \.self) { remote in
                    HStack {
                        Label(remote, systemImage: "arrow.triangle.branch")
                        Spacer()
                        Text("any folder").foregroundStyle(Brand.muted)
                    }
                }
                ForEach(model.config?.pinned ?? [], id: \.self) { folder in
                    HStack {
                        Text(tildePath(folder))
                        if let until = model.config?.pinnedUntil?[folder] {
                            let date = Date(timeIntervalSince1970: TimeInterval(until))
                            Text(date < Date() ? "ended" : "until \(date.formatted(date: .abbreviated, time: .omitted))")
                                .foregroundStyle(Brand.muted)
                        }
                        Spacer()
                        Button("Unpin") { Task { await model.setPinned(folder, false) } }
                    }
                }
            }
        }
        .formStyle(.grouped)
        .task {
            await model.loadConfig()
            if let c = model.config { idle = c.idleDays; dormant = c.dormantDays }
        }
    }

    private func pickFolder() -> URL? {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        return panel.runModal() == .OK ? panel.url : nil
    }
}
