import SwiftUI
import AppKit
import CleanCore

/// What is installed for development. Read-only: AgentClean shows where
/// things are and the tool's own command for removing them; it runs nothing.
struct DevStackView: View {
    @Environment(AppModel.self) private var model
    @State private var group: DevStackItem.Group?
    @State private var search = ""

    private var rows: [DevStackItem] {
        model.devStack
            .filter { group == nil || $0.group == group }
            .filter { search.isEmpty || $0.name.localizedCaseInsensitiveContains(search) || $0.path.localizedCaseInsensitiveContains(search) }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                ScreenHeader(title: "Dev Stack", subtitle: "Databases, runtimes, global tools, SDKs and VM disks on this Mac. Read-only.")
                Spacer()
                TextField("Search", text: $search).textFieldStyle(.roundedBorder).frame(width: 180)
                Button { model.loadDevStack() } label: {
                    if model.devStackLoading { ProgressView().controlSize(.small) } else { Label("Refresh", systemImage: "arrow.clockwise") }
                }
                .buttonStyle(SecondaryButtonStyle())
                .disabled(model.devStackLoading)
            }
            Picker("", selection: $group) {
                Text("All").tag(DevStackItem.Group?.none)
                ForEach(DevStackItem.Group.allCases, id: \.self) { Text($0.displayName).tag(Optional($0)) }
            }
            .pickerStyle(.menu).labelsHidden().fixedSize()
            .accessibilityIdentifier("devstack.group")

            if model.devStackLoading && model.devStack.isEmpty {
                Spacer()
                ProgressView("Looking at what's installed…").frame(maxWidth: .infinity)
                Spacer()
            } else {
                ScrollView {
                    LazyVStack(spacing: 6) {
                        ForEach(DevStackItem.Group.allCases, id: \.self) { g in
                            let items = rows.filter { $0.group == g }
                            if !items.isEmpty {
                                HStack {
                                    Image(systemName: g.systemImage).foregroundStyle(Brand.sky).accessibilityHidden(true)
                                    SectionLabel(text: g.displayName)
                                    Spacer()
                                    Text(bytes(items.reduce(0) { $0 + $1.bytes })).font(.caption.monospacedDigit()).foregroundStyle(Brand.muted)
                                }
                                .padding(.top, 10)
                                ForEach(items) { row($0) }
                            }
                        }
                    }
                }
            }
        }
        .padding(28)
        .task { if model.devStack.isEmpty { model.loadDevStack() } }
    }

    private func row(_ item: DevStackItem) -> some View {
        HStack(alignment: .top, spacing: 12) {
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 8) {
                    Text(item.name).font(.headline)
                    Text(item.status.rawValue).font(.caption).foregroundStyle(item.status == .running ? Brand.green : Brand.muted)
                }
                Text(item.detail).font(.caption).foregroundStyle(Brand.muted)
                if case .command(let cmd) = item.removal {
                    HStack(spacing: 6) {
                        Text("Remove with:").font(.caption).foregroundStyle(Brand.muted)
                        Text(cmd).font(.caption.monospaced()).textSelection(.enabled)
                        Button {
                            NSPasteboard.general.clearContents()
                            NSPasteboard.general.setString(cmd, forType: .string)
                        } label: { Image(systemName: "doc.on.doc") }
                            .accessibilityLabel("Copy the command \(cmd)")
                            .buttonStyle(.borderless).help("Copy the command")
                    }
                }
                if let hint = item.hint { Text(hint).font(.caption).foregroundStyle(Brand.muted) }
            }
            Spacer()
            Text(item.bytes > 0 ? bytes(item.bytes) : "").monospacedDigit()
            if item.path.hasPrefix("/") {
                Button {
                    NSWorkspace.shared.selectFile(item.path, inFileViewerRootedAtPath: "")
                } label: { Image(systemName: "folder") }
                    .accessibilityLabel("Show \(item.name) in Finder")
                    .buttonStyle(.borderless).help("Show in Finder")
            }
        }
        .padding(12)
        .glassCard(radius: 12)
    }
}
