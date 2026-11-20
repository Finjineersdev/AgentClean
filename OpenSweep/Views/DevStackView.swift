import SwiftUI
import AppKit
import CleanCore

/// What is installed for development. Read-only: OpenSweep shows where
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
                    .accessibilityLabel("Search tools")
                Button { model.loadDevStack() } label: {
                    if model.devStackLoading { ProgressView().controlSize(.small) } else { Label("Refresh", systemImage: "arrow.clockwise") }
                }
                .buttonStyle(SecondaryButtonStyle())
                .disabled(model.devStackLoading)
            }
            // A Menu, not a menu Picker: the picker's pop-up has no press action for VoiceOver.
            Menu(group?.displayName ?? "All") {
                Button("All") { group = nil }
                ForEach(DevStackItem.Group.allCases, id: \.self) { g in Button(g.displayName) { group = g } }
            }
            .fixedSize()
            .accessibilityLabel("Show")
            .accessibilityValue(group?.displayName ?? "All")
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
                                SectionLabel(text: g.displayName, trailing: bytes(items.reduce(0) { $0 + $1.bytes }))
                                    .padding(.top, 14).padding(.bottom, 2)
                                ForEach(Array(items.enumerated()), id: \.element.id) { i, item in row(item).entrance(i + 1) }
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
            EcosystemBadge(ecosystem: Ecosystem.of(tool: item.name)
                ?? Ecosystem(id: item.name, name: item.group.displayName, logo: nil, color: Brand.sky, symbol: item.group.systemImage), size: 30)
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 8) {
                    Text(item.name).font(.headline).foregroundStyle(Brand.text)
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
        .padding(.horizontal, 14).padding(.vertical, 12)
        .glassCard()
    }
}
