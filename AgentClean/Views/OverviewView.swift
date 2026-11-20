import SwiftUI
import AgentEngine

/// Where the disk went, by language, and what can come back now.
struct OverviewView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 28) {
                HStack(alignment: .top) {
                    ScreenHeader(title: "Overview", subtitle: subtitle).lineLimit(2).truncationMode(.middle)
                    Spacer(minLength: 24)
                    Button(model.scanning ? "Stop" : (model.lastScan == nil ? "Scan projects" : "Scan again")) {
                        model.toggleScan()
                    }
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut("r", modifiers: .command)
                    .accessibilityIdentifier("scan")
                }

                hero

                if !model.cleanable.isEmpty {
                    VStack(alignment: .leading, spacing: 10) {
                        SectionLabel(text: "Biggest projects", trailing: "\(model.projects.count) found")
                        let top = Array(model.cleanable.prefix(6))
                        let biggest = top.map(model.total).max() ?? 1
                        VStack(spacing: 0) {
                            ForEach(Array(top.enumerated()), id: \.element.root) { i, p in
                                Button {
                                    model.selectedProject = p.root
                                    model.screen = .projects
                                } label: {
                                    ProjectRow(project: p, total: model.total(p), share: Double(model.total(p)) / Double(max(biggest, 1)))
                                }
                                .buttonStyle(RowButtonStyle())
                                if i < top.count - 1 { Divider().overlay(Brand.hairline).padding(.leading, 64) }
                            }
                        }
                        .glassCard()
                    }
                }

                if !model.skipped.isEmpty {
                    DisclosureGroup("\(model.skipped.count) folders were not scanned") {
                        ForEach(model.skipped, id: \.path) { s in
                            HStack {
                                Text((s.path as NSString).abbreviatingWithTildeInPath).font(.caption.monospaced())
                                    .lineLimit(1).truncationMode(.middle)
                                Spacer()
                                Text(s.detail).font(.caption)
                            }
                            .foregroundStyle(Brand.muted)
                        }
                    }
                    .foregroundStyle(Brand.muted)
                }
            }
            .padding(.horizontal, 32).padding(.vertical, 28)
            .frame(maxWidth: 1000, alignment: .leading)
            .frame(maxWidth: .infinity)
        }
    }

    private var subtitle: String {
        let roots = model.scanRoots.map { ($0 as NSString).abbreviatingWithTildeInPath }.joined(separator: ", ")
        if model.scanning { return "Looking in \(roots)…" }
        if let last = model.lastScan { return "Scanned \(last.relative) in \(roots). Cleaning moves folders to a quarantine you can undo." }
        return "Finds rebuildable folders in \(roots). Nothing is deleted: cleaning moves folders to a quarantine you can restore from."
    }

    // MARK: Hero: the disk bar

    @ViewBuilder
    private var hero: some View {
        VStack(alignment: .leading, spacing: 18) {
            if model.scanning && model.summary == nil {
                Text("\(model.projects.count) projects so far").font(.figure(40)).foregroundStyle(Brand.text)
                ProgressView().progressViewStyle(.linear).tint(Brand.teal)
            } else if let s = model.summary {
                HStack(alignment: .firstTextBaseline, spacing: 12) {
                    Text(bytes(s.ready.bytes)).font(.figure(48)).foregroundStyle(Brand.text)
                        .accessibilityIdentifier("ready.bytes")
                    Text("can be cleaned now, \(s.ready.folders) folders that rebuild on their own")
                        .font(.title3).foregroundStyle(Brand.muted)
                }
                DiskBar(segments: segments, caution: s.caution.bytes, active: s.active.bytes)
                legend(s)
            } else {
                Text("Nothing scanned yet").font(.figure(36)).foregroundStyle(Brand.text)
                Text("A scan reads your project folders and measures build output, dependencies and caches. It changes nothing.")
                    .foregroundStyle(Brand.muted)
                SettingsLink { Text("Choose folders…") }.buttonStyle(SecondaryButtonStyle())
            }
        }
        .padding(24)
        .frame(maxWidth: .infinity, alignment: .leading)
        .glassCard(radius: 16)
    }

    /// Ready bytes by language: Safe folders in projects that are not active or pinned.
    private var segments: [(Ecosystem, UInt64)] {
        var by: [String: UInt64] = [:]
        for p in model.projects where !p.pinned && p.verdict != .active {
            for i in p.items where i.kind == .safe && i.refused == nil { by[i.rule, default: 0] += i.bytes ?? 0 }
        }
        return by.filter { $0.value > 0 }.sorted { $0.value > $1.value }.map { (Ecosystem.of(rule: $0.key), $0.value) }
    }

    private func legend(_ s: ScanSummary) -> some View {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 170), spacing: 12, alignment: .leading)], alignment: .leading, spacing: 12) {
            ForEach(segments, id: \.0.id) { e, b in
                HStack(spacing: 10) {
                    EcosystemBadge(ecosystem: e, size: 28)
                    VStack(alignment: .leading, spacing: 1) {
                        Text(e.name).font(.callout.weight(.medium)).foregroundStyle(Brand.text)
                        Text(bytes(b)).font(.callout).foregroundStyle(Brand.muted).monospacedDigit()
                    }
                }
                .accessibilityElement(children: .combine)
            }
            if s.caution.bytes > 0 {
                keyItem(swatch: AnyView(Hatch(color: Brand.amber).frame(width: 28, height: 28).clipShape(RoundedRectangle(cornerRadius: 8))),
                        title: "Needs a reinstall", detail: "\(bytes(s.caution.bytes)), never ticked for you")
            }
            if s.active.bytes > 0 {
                keyItem(swatch: AnyView(RoundedRectangle(cornerRadius: 8).strokeBorder(Brand.sky, style: StrokeStyle(lineWidth: 1.5, dash: [3, 2])).frame(width: 28, height: 28)),
                        title: "In active projects", detail: "\(bytes(s.active.bytes)), only on your say-so")
            }
        }
    }

    private func keyItem(swatch: AnyView, title: String, detail: String) -> some View {
        HStack(spacing: 10) {
            swatch.accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 1) {
                Text(title).font(.callout.weight(.medium)).foregroundStyle(Brand.text)
                Text(detail).font(.callout).foregroundStyle(Brand.muted).monospacedDigit()
            }
        }
        .accessibilityElement(children: .combine)
    }
}

/// One bar for everything the scan measured: ready space by language in each
/// language's colour, then folders that need a reinstall (hatched), then
/// folders in active projects (outlined).
struct DiskBar: View {
    let segments: [(Ecosystem, UInt64)]
    let caution: UInt64
    let active: UInt64

    var body: some View {
        let ready = segments.reduce(0) { $0 + $1.1 }
        let total = Double(max(ready + caution + active, 1))
        GeometryReader { g in
            let gaps = CGFloat(segments.count + (caution > 0 ? 1 : 0) + (active > 0 ? 1 : 0) - 1) * 3
            let width = max(g.size.width - max(gaps, 0), 0)
            HStack(spacing: 3) {
                ForEach(segments, id: \.0.id) { e, b in
                    RoundedRectangle(cornerRadius: 5, style: .continuous).fill(e.color)
                        .frame(width: max(4, width * Double(b) / total))
                        .help("\(e.name): \(bytes(b))")
                }
                if caution > 0 {
                    Hatch(color: Brand.amber).clipShape(RoundedRectangle(cornerRadius: 5, style: .continuous))
                        .frame(width: max(4, width * Double(caution) / total))
                        .help("Needs a reinstall: \(bytes(caution))")
                }
                if active > 0 {
                    RoundedRectangle(cornerRadius: 5, style: .continuous)
                        .strokeBorder(Brand.sky, style: StrokeStyle(lineWidth: 1.5, dash: [3, 2]))
                        .frame(width: max(4, width * Double(active) / total))
                        .help("In active projects: \(bytes(active))")
                }
            }
        }
        .frame(height: 22)
        .accessibilityElement()
        .accessibilityLabel("Space by language")
        .accessibilityValue(segments.map { "\($0.0.name) \(bytes($0.1))" }.joined(separator: ", "))
    }
}

/// Diagonal stripes for space that needs a reinstall.
struct Hatch: View {
    let color: Color
    var body: some View {
        Canvas { ctx, size in
            ctx.fill(Path(CGRect(origin: .zero, size: size)), with: .color(color.opacity(0.18)))
            var x: CGFloat = -size.height
            while x < size.width {
                var p = Path()
                p.move(to: CGPoint(x: x, y: size.height))
                p.addLine(to: CGPoint(x: x + size.height, y: 0))
                ctx.stroke(p, with: .color(color.opacity(0.85)), lineWidth: 1.5)
                x += 6
            }
        }
    }
}

/// A full-width row that highlights on hover and press.
struct RowButtonStyle: ButtonStyle {
    var selected = false
    @State private var hover = false
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .background(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .fill(selected ? Brand.teal.opacity(0.12) : (configuration.isPressed ? Brand.raised : (hover ? Brand.raised.opacity(0.6) : .clear)))
                    .padding(2))
            .onHover { hover = $0 }
    }
}

struct ProjectRow: View {
    let project: Project
    let total: UInt64
    var share: Double = 0

    var body: some View {
        HStack(spacing: 14) {
            EcosystemStack(ecosystems: project.ecosystems.isEmpty ? [Ecosystem.of(rule: "")] : project.ecosystems)
            VStack(alignment: .leading, spacing: 3) {
                Text(URL(fileURLWithPath: project.root).lastPathComponent).font(.body.weight(.semibold)).foregroundStyle(Brand.text)
                Text((project.root as NSString).abbreviatingWithTildeInPath)
                    .font(.caption.monospaced()).foregroundStyle(Brand.muted).lineLimit(1).truncationMode(.middle)
            }
            Spacer(minLength: 12)
            if project.pinned {
                StatusChip(text: "Pinned", systemImage: "pin.fill", tint: Brand.slate)
            } else {
                StatusChip(text: project.verdict.label, systemImage: project.verdict.symbol, tint: project.verdict.tint)
            }
            VStack(alignment: .trailing, spacing: 5) {
                Text(total == 0 ? "—" : bytes(total)).font(.callout.weight(.semibold)).monospacedDigit().foregroundStyle(Brand.text)
                ShareBar(share: share, tint: project.ecosystems.first?.color ?? Brand.teal).frame(width: 72)
            }
            .frame(width: 84, alignment: .trailing)
        }
        .padding(.horizontal, 14).padding(.vertical, 11)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }
}
