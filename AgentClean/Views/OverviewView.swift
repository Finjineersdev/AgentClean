import SwiftUI
import AgentEngine

struct OverviewView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                ScreenHeader(
                    title: "Overview",
                    subtitle: "Find rebuildable folders in your projects. Nothing is deleted: cleaning moves folders to a quarantine you can restore from.")

                HStack(spacing: 32) {
                    BigScanButton(title: model.lastScan == nil ? "Scan" : "Rescan", isBusy: model.scanning) {
                        model.toggleScan()
                    }
                    .accessibilityIdentifier("scan")
                    VStack(alignment: .leading, spacing: 8) {
                        if model.scanning {
                            Text("Looking for projects…").font(.title3.weight(.semibold))
                            Text("\(model.projects.count) found so far").foregroundStyle(.secondary).monospacedDigit()
                        } else if let s = model.summary {
                            Text(bytes(s.ready.bytes)).font(.system(size: 40, weight: .bold, design: .rounded))
                                .accessibilityIdentifier("ready.bytes")
                            Text("ready to clean in \(s.ready.folders) Safe folders, across \(s.projects) projects")
                                .foregroundStyle(.secondary)
                        } else {
                            Text("Scan your projects").font(.title3.weight(.semibold))
                            Text("Looks in: " + model.scanRoots.map { ($0 as NSString).abbreviatingWithTildeInPath }.joined(separator: ", "))
                                .foregroundStyle(.secondary)
                            SettingsLink { Text("Change folders…") }.buttonStyle(SecondaryButtonStyle())
                        }
                    }
                }
                .padding(24)
                .frame(maxWidth: .infinity, alignment: .leading)
                .glassCard()

                if let s = model.summary {
                    HStack(spacing: 14) {
                        bucket("Ready", s.ready, Brand.green, "Safe folders in projects not in use. Rebuilt automatically.")
                        bucket("Caution", s.caution, Brand.amber, "Need a reinstall (node_modules, .venv). Never ticked for you.")
                        bucket("In active projects", s.active, Brand.sky, "Recently edited or in use. Cleaning them needs your say-so.")
                    }
                }

                if !model.cleanable.isEmpty {
                    SectionLabel(text: "Biggest projects")
                    VStack(spacing: 8) {
                        ForEach(model.cleanable.prefix(6), id: \.root) { p in
                            Button {
                                model.selectedProject = p.root
                                model.screen = .projects
                            } label: {
                                ProjectRow(project: p, total: model.total(p))
                            }
                            .buttonStyle(.plain)
                        }
                    }
                }

                if !model.skipped.isEmpty {
                    DisclosureGroup("\(model.skipped.count) folders were skipped") {
                        ForEach(model.skipped, id: \.path) { s in
                            HStack {
                                Text((s.path as NSString).abbreviatingWithTildeInPath).lineLimit(1).truncationMode(.middle)
                                Spacer()
                                Text(s.detail).foregroundStyle(.secondary)
                            }
                            .font(.caption)
                        }
                    }
                    .foregroundStyle(.secondary)
                }
            }
            .padding(28)
        }
    }

    private func bucket(_ title: String, _ b: Bucket, _ tint: Color, _ note: String) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title).font(.headline).foregroundStyle(tint)
            Text(bytes(b.bytes)).font(.title2.weight(.bold)).monospacedDigit()
            Text("\(b.folders) folders").font(.caption).foregroundStyle(.secondary)
            Text(note).font(.caption).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .topLeading)
        .glassCard(radius: 14)
    }
}

struct ProjectRow: View {
    let project: Project
    let total: UInt64

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: project.pinned ? "pin.fill" : project.verdict.symbol)
                .foregroundStyle(project.pinned ? Brand.slate : project.verdict.tint)
                .frame(width: 20)
            VStack(alignment: .leading, spacing: 2) {
                Text(URL(fileURLWithPath: project.root).lastPathComponent).font(.headline)
                Text((project.root as NSString).abbreviatingWithTildeInPath)
                    .font(.caption).foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle)
            }
            Spacer()
            Text(project.pinned ? "Pinned" : project.verdict.label)
                .font(.caption.weight(.semibold))
                .foregroundStyle(project.pinned ? Brand.slate : project.verdict.tint)
            Text(total == 0 ? "—" : bytes(total)).font(.callout.weight(.semibold)).monospacedDigit().frame(width: 90, alignment: .trailing)
        }
        .padding(.horizontal, 14).padding(.vertical, 10)
        .contentShape(Rectangle())
        .glassCard(radius: 12)
    }
}
