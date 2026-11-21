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
                    Text("No known tool caches on this Mac.").foregroundStyle(Brand.muted).padding(.top, 40).frame(maxWidth: .infinity)
                }

                if !results.isEmpty { resultCard }

                if model.caches.contains(where: { $0.bytes > 0 }) {
                    CacheMap(caches: model.caches, picked: $picked, running: running)
                        .entrance(0)
                }

                if !clearable.isEmpty {
                    SectionLabel(text: "Can be cleared")
                    ForEach(Array(clearable.enumerated()), id: \.element.id) { i, c in row(c).entrance(i + 1) }
                    HStack {
                        Text(picked.isEmpty ? "Tick a cache to clear it" : "\(picked.count) selected, \(bytes(pickedBytes))")
                            .foregroundStyle(Brand.muted).monospacedDigit()
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
                    ForEach(Array(measured.enumerated()), id: \.element.id) { i, c in row(c).entrance(i + 2) }
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
            Text("OpenSweep will run:\n"
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
            EcosystemBadge(ecosystem: Ecosystem.of(cache: c.id), size: 30)
            VStack(alignment: .leading, spacing: 3) {
                Text(c.name).font(.headline).foregroundStyle(Brand.text)
                Text(tildePath(c.path)).font(.caption).foregroundStyle(Brand.muted)
                    .lineLimit(1).truncationMode(.middle)
                if let cmd = c.command, !cmd.isEmpty {
                    Text(c.installed ? "Runs: \(cmd.joined(separator: " "))" : "\(c.tool) is not installed, so it cannot be cleared from here.")
                        .font(.caption.monospaced()).foregroundStyle(Brand.muted)
                }
                Text(c.note).font(.caption).foregroundStyle(Brand.muted).fixedSize(horizontal: false, vertical: true)
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

/// Where cache space goes: one block per cache, sized by bytes. A block that
/// can be cleared ticks it, like its checkbox; measured-only ones are dimmed.
struct CacheMap: View {
    let caches: [ToolCache]
    @Binding var picked: Set<String>
    let running: Bool
    @State private var hovered: String?

    private var shown: [ToolCache] { caches.filter { $0.bytes > 0 }.sorted { $0.bytes > $1.bytes } }
    private var total: UInt64 { shown.reduce(0) { $0 + $1.bytes } }
    private var clearableBytes: UInt64 { shown.filter(\.clearable).reduce(0) { $0 + $1.bytes } }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .firstTextBaseline, spacing: 10) {
                Text(bytes(total)).font(.figure(28)).foregroundStyle(Brand.text)
                Text("in caches, \(bytes(clearableBytes)) can be cleared here").foregroundStyle(Brand.muted)
                Spacer()
                Text(hovered ?? "Pick a block to tick it").font(.callout).foregroundStyle(Brand.muted)
                    .lineLimit(1).truncationMode(.middle)
            }
            .accessibilityElement(children: .combine)
            GeometryReader { g in
                let rects = Treemap.squarify(shown.map { Double($0.bytes) }, in: CGRect(origin: .zero, size: g.size))
                ZStack(alignment: .topLeading) {
                    ForEach(Array(shown.enumerated()), id: \.element.id) { i, c in
                        block(c, in: rects[i].insetBy(dx: 2, dy: 2))
                    }
                }
            }
            .frame(height: 220)
            .accessibilityElement(children: .contain)
            .accessibilityLabel("Cache map")
            HStack(spacing: 16) {
                legend(dashed: false, "Can be cleared here")
                legend(dashed: true, "Measured only: clear it with the tool itself")
            }
            .font(.caption).foregroundStyle(Brand.muted)
        }
        .padding(16)
        .glassCard(radius: 14)
    }

    private func legend(dashed: Bool, _ text: String) -> some View {
        HStack(spacing: 6) {
            RoundedRectangle(cornerRadius: 3).fill(Brand.muted.opacity(dashed ? 0.12 : 0.35))
                .overlay(RoundedRectangle(cornerRadius: 3).strokeBorder(Brand.muted.opacity(dashed ? 0.6 : 0), style: StrokeStyle(lineWidth: 1, dash: [3, 2])))
                .frame(width: 14, height: 10).accessibilityHidden(true)
            Text(text)
        }
    }

    private func block(_ c: ToolCache, in r: CGRect) -> some View {
        let tint = Ecosystem.of(cache: c.id).color
        let on = picked.contains(c.id)
        let share = total > 0 ? Int((Double(c.bytes) / Double(total) * 100).rounded()) : 0
        let shape = RoundedRectangle(cornerRadius: 8, style: .continuous)
        return Button {
            guard c.clearable, !running else { return }
            if on { picked.remove(c.id) } else { picked.insert(c.id) }
        } label: {
            ZStack(alignment: .topLeading) {
                shape.fill(tint.opacity(c.clearable ? (hovered == c.name || on ? 0.50 : 0.30) : 0.12))
                shape.strokeBorder(on ? tint : (c.clearable ? .clear : tint.opacity(0.4)),
                                   style: StrokeStyle(lineWidth: on ? 2 : 1, dash: c.clearable ? [] : [4, 3]))
                if r.width > 80 && r.height > 40 {
                    VStack(alignment: .leading, spacing: 1) {
                        HStack(spacing: 4) {
                            if on { Image(systemName: "checkmark.circle.fill").accessibilityHidden(true) }
                            Text(c.name).font(.caption.weight(.semibold)).lineLimit(1)
                        }
                        Text("\(bytes(c.bytes)) · \(share)%").font(.caption2).monospacedDigit().opacity(0.8)
                    }
                    .foregroundStyle(Brand.text).padding(7)
                }
                if r.width > 44 && r.height > 44 {
                    EcosystemBadge(ecosystem: Ecosystem.of(cache: c.id), size: 20).padding(6)
                        .frame(width: r.width, height: r.height, alignment: .bottomTrailing)
                        .allowsHitTesting(false)
                }
            }
        }
        .buttonStyle(.plain)
        .frame(width: max(r.width, 0), height: max(r.height, 0))
        .offset(x: r.minX, y: r.minY)
        .onHover { hovered = $0 ? "\(c.name): \(bytes(c.bytes))\(c.clearable ? "" : ", measured only")" : nil }
        .accessibilityLabel("\(c.name), \(bytes(c.bytes)), \(share) percent\(c.clearable ? "" : ", measured only")")
        .accessibilityAddTraits(on ? .isSelected : [])
        .accessibilityIdentifier("cachemap.\(c.id)")
        .animation(.smooth(duration: 0.22), value: hovered)
        .animation(.smooth(duration: 0.22), value: on)
    }
}
