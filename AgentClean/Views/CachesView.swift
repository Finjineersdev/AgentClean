import SwiftUI
import AppKit
import AgentEngine

/// Caches shared by every project (ADR 0006). Each is cleared by the tool's
/// own command, run by the engine; nothing is ticked for you, and clearing
/// cannot be undone (the tool downloads again what it needs).
struct CachesView: View {
    @Environment(AppModel.self) private var model
    @State private var picked: Set<String> = []
    @State private var confirming = false
    @State private var running = false
    @State private var results: [ToolCache] = []

    private var clearable: [ToolCache] { model.caches.filter(\.clearable) }
    private var measured: [ToolCache] { model.caches.filter { !$0.clearable } }
    private var pickedBytes: UInt64 { model.caches.filter { picked.contains($0.id) }.reduce(0) { $0 + $1.bytes } }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                HStack(alignment: .top) {
                    ScreenHeader(
                        title: "Caches",
                        subtitle: "Downloads shared by all your projects. Each is cleared by the tool's own command. Not undoable: the tool downloads again what it needs.")
                    Spacer()
                    Button { Task { await model.loadCaches() } } label: {
                        if model.cachesLoading { ProgressView().controlSize(.small) } else { Label("Refresh", systemImage: "arrow.clockwise") }
                    }
                    .buttonStyle(SecondaryButtonStyle())
                    .disabled(model.cachesLoading || running)
                }

                if model.caches.isEmpty && !model.cachesLoading {
                    Text("No known tool caches on this Mac.").foregroundStyle(.secondary).padding(.top, 40).frame(maxWidth: .infinity)
                }

                if !results.isEmpty { resultCard }

                if !clearable.isEmpty {
                    SectionLabel(text: "Can be cleared")
                    ForEach(clearable) { row($0) }
                    HStack {
                        Text("\(picked.count) selected · \(bytes(pickedBytes))")
                            .foregroundStyle(.secondary).monospacedDigit()
                            .accessibilityIdentifier("caches.summary")
                        Spacer()
                        Button(running ? "Clearing…" : "Clear…") { confirming = true }
                            .buttonStyle(PrimaryButtonStyle())
                            .disabled(picked.isEmpty || running)
                            .accessibilityIdentifier("caches.clear")
                    }
                }

                if !measured.isEmpty {
                    SectionLabel(text: "Measured only").padding(.top, 8)
                    ForEach(measured) { row($0) }
                }
            }
            .padding(28)
        }
        .task { if model.caches.isEmpty { await model.loadCaches() } }
        .confirmationDialog(
            "Clear \(bytes(pickedBytes)) of caches?",
            isPresented: $confirming
        ) {
            Button("Clear", role: .destructive) { clear() }
                .accessibilityIdentifier("caches.confirm")
        } message: {
            Text("AgentClean will run:\n"
                + model.caches.filter { picked.contains($0.id) }.map { ($0.command ?? []).joined(separator: " ") }.joined(separator: "\n")
                + "\n\nThis cannot be undone; tools download again what they need.")
        }
    }

    private func clear() {
        let ids = picked.sorted()
        running = true
        Task {
            results = await model.clearCaches(ids)
            picked = []
            running = false
        }
    }

    private var resultCard: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(results) { r in
                HStack {
                    Image(systemName: r.status == 0 ? "checkmark.circle.fill" : "exclamationmark.triangle.fill")
                        .foregroundStyle(r.status == 0 ? Brand.green : Brand.amber)
                        .accessibilityHidden(true)
                    switch r.status {
                    case 0: Text("\(r.name): freed \(bytes(r.freed ?? 0))")
                    case .some(let code): Text("\(r.name): the command failed (exit \(code)). \(r.stderr ?? "")")
                    case .none: Text(r.error.map { "\(r.name): \($0)" } ?? "\(r.name): the command took too long and was stopped.")
                    }
                }
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .glassCard(radius: 12)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("caches.result")
    }

    private func row(_ c: ToolCache) -> some View {
        HStack(alignment: .top, spacing: 12) {
            if c.clearable {
                Toggle("", isOn: Binding(
                    get: { picked.contains(c.id) },
                    set: { if $0 { picked.insert(c.id) } else { picked.remove(c.id) } }))
                    .toggleStyle(.checkbox).labelsHidden()
                    .accessibilityLabel("Clear \(c.name)")
                    .accessibilityIdentifier("cache.\(c.id)")
                    .disabled(running)
            }
            VStack(alignment: .leading, spacing: 3) {
                Text(c.name).font(.headline)
                Text((c.path as NSString).abbreviatingWithTildeInPath).font(.caption).foregroundStyle(.secondary)
                    .lineLimit(1).truncationMode(.middle)
                if let cmd = c.command, !cmd.isEmpty {
                    Text(c.installed ? "Runs: \(cmd.joined(separator: " "))" : "\(c.tool) is not installed, so it cannot be cleared from here.")
                        .font(.caption.monospaced()).foregroundStyle(.secondary)
                }
                Text(c.note).font(.caption).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            }
            Spacer()
            Text(bytes(c.bytes)).monospacedDigit()
            Button {
                NSWorkspace.shared.selectFile(c.path, inFileViewerRootedAtPath: "")
            } label: { Image(systemName: "folder") }
                .buttonStyle(.borderless).help("Show in Finder")
                .accessibilityLabel("Show \(c.name) in Finder")
        }
        .padding(12)
        .glassCard(radius: 12)
    }
}
