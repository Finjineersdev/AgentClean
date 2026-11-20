import SwiftUI
import AppKit
import AgentEngine

struct SettingsView: View {
    @Environment(AppModel.self) private var model
    @State private var idle = 14
    @State private var dormant = 90
    @AppStorage("showMenuBar") private var showMenuBar = true

    var body: some View {
        Form {
            Section {
                Toggle("Show AgentClean in the menu bar", isOn: $showMenuBar)
                    .accessibilityIdentifier("settings.menuBar")
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
                            Text((root as NSString).abbreviatingWithTildeInPath)
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
                    .foregroundStyle(.secondary)
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
                Toggle("Rescan when AgentClean opens", isOn: $model.scanOnOpen)
                    .accessibilityIdentifier("settings.scanOnOpen")
            } header: {
                Text("Space and scans")
            } footer: {
                Text("Cleaned folders wait in quarantine so you can undo; after this long they are deleted for good and the space comes back. The last scan is kept, and refreshed in the background when it is over an hour old.")
                    .foregroundStyle(.secondary)
            }

            Section {
                Stepper("Active if changed within \(idle) days", value: $idle, in: 1...dormant)
                Stepper("Dormant after \(dormant) days", value: $dormant, in: idle...3650)
                Button("Save") { Task { await model.setDays(idle: idle, dormant: dormant) } }
                    .disabled(idle == model.config?.idleDays && dormant == model.config?.dormantDays)
            } header: {
                Text("Activity")
            } footer: {
                Text("Active projects are only cleaned when you say so in the project.").foregroundStyle(.secondary)
            }

            Section("Never scan") {
                ForEach(model.config?.exclude ?? [], id: \.self) { folder in
                    HStack {
                        Text((folder as NSString).abbreviatingWithTildeInPath)
                        Spacer()
                        Button("Remove") { Task { await model.include(folder) } }
                    }
                }
                Button("Add Folder…") { if let url = pickFolder() { Task { await model.exclude(url) } } }
            }

            Section("Pinned: never cleaned") {
                if (model.config?.pinned.isEmpty ?? true) && (model.config?.pinnedRemotes ?? []).isEmpty {
                    Text("Nothing is pinned. Pin a project from its page.").foregroundStyle(.secondary)
                }
                ForEach(model.config?.pinnedRemotes ?? [], id: \.self) { remote in
                    HStack {
                        Label(remote, systemImage: "arrow.triangle.branch")
                        Spacer()
                        Text("any folder").foregroundStyle(.secondary)
                    }
                }
                ForEach(model.config?.pinned ?? [], id: \.self) { folder in
                    HStack {
                        Text((folder as NSString).abbreviatingWithTildeInPath)
                        if let until = model.config?.pinnedUntil?[folder] {
                            let date = Date(timeIntervalSince1970: TimeInterval(until))
                            Text(date < Date() ? "ended" : "until \(date.formatted(date: .abbreviated, time: .omitted))")
                                .foregroundStyle(.secondary)
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
