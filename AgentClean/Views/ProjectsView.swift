import SwiftUI
import AgentEngine

struct ProjectsView: View {
    @Environment(AppModel.self) private var model
    @State private var search = ""

    private var rows: [AgentEngine.Project] {
        model.projects
            .filter { search.isEmpty || $0.root.localizedCaseInsensitiveContains(search) }
            .sorted { model.total($0) > model.total($1) }
    }

    var body: some View {
        HStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 14) {
                ScreenHeader(title: "Projects", subtitle: "\(model.projects.count) found, biggest first")
                HStack(spacing: 8) {
                    Image(systemName: "magnifyingglass").foregroundStyle(Brand.muted).accessibilityHidden(true)
                    TextField("Search", text: $search).textFieldStyle(.plain)
                        .accessibilityLabel("Search projects")
                }
                .padding(.horizontal, 10).padding(.vertical, 7)
                .glassCard(radius: 9)
                if model.projects.isEmpty {
                    Spacer()
                    Text(model.scanning ? "Scanning…" : "Run a scan from Overview to find projects.")
                        .foregroundStyle(Brand.muted).frame(maxWidth: .infinity)
                    Spacer()
                } else {
                    ScrollView {
                        let biggest = rows.map(model.total).max() ?? 1
                        LazyVStack(spacing: 2) {
                            ForEach(rows, id: \.root) { p in
                                Button { model.selectedProject = p.root } label: {
                                    CompactProjectRow(project: p, total: model.total(p), share: Double(model.total(p)) / Double(max(biggest, 1)))
                                }
                                .buttonStyle(RowButtonStyle(selected: model.selectedProject == p.root))
                                .accessibilityIdentifier("project.\(URL(fileURLWithPath: p.root).lastPathComponent)")
                            }
                        }
                        .accessibilityElement(children: .contain)
                        .accessibilityLabel("Projects")
                    }
                }
            }
            .padding(.horizontal, 20).padding(.vertical, 24)
            .frame(width: 360)
            .background(Brand.canvas)

            Divider().overlay(Brand.hairline)

            if let p = model.project(model.selectedProject) {
                // Keyed by path, not value: a refresh after a clean must not tear down
                // the open Review sheet (and its Undo).
                ProjectDetail(project: p).id(p.root)
            } else {
                VStack(spacing: 8) {
                    Image(systemName: "sidebar.left").font(.largeTitle).foregroundStyle(Brand.hairline).accessibilityHidden(true)
                    Text("Choose a project to see its folders").foregroundStyle(Brand.muted)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
    }
}

/// One project's folders, each ticked by hand. Safe folders start ticked in
/// projects not in use; Caution folders never do (F14).
struct ProjectDetail: View {
    @Environment(AppModel.self) private var model
    let project: AgentEngine.Project
    @State private var picked: Set<String> = []
    @State private var allowActive = false
    @State private var review: CleanOutput?
    @State private var busy = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                HStack(alignment: .center, spacing: 14) {
                    EcosystemBadge(ecosystem: project.ecosystems.first ?? Ecosystem.of(rule: ""), size: 44)
                    VStack(alignment: .leading, spacing: 4) {
                        Text(URL(fileURLWithPath: project.root).lastPathComponent)
                            .font(.system(size: 26, weight: .bold)).tracking(-0.4).foregroundStyle(Brand.text)
                        Text((project.root as NSString).abbreviatingWithTildeInPath).font(.callout.monospaced()).foregroundStyle(Brand.muted)
                            .lineLimit(1).truncationMode(.middle).help(project.root)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    Button(project.pinned ? "Unpin" : "Pin") {
                        Task { await model.setPinned(project.root, !project.pinned) }
                    }
                    .buttonStyle(SecondaryButtonStyle())
                    .help(project.pinned ? "Allow cleaning this project again" : "Never clean this project")
                    .accessibilityIdentifier("pin")
                }

                HStack(spacing: 8) {
                    if project.pinned {
                        StatusChip(text: "Pinned", systemImage: "pin.fill", tint: Brand.slate)
                    } else {
                        StatusChip(text: project.verdict.label, systemImage: project.verdict.symbol, tint: project.verdict.tint)
                    }
                    Text(project.reason).foregroundStyle(Brand.muted)
                    Spacer()
                    if project.ecosystems.count > 1 {
                        Text(project.ecosystems.map(\.name).joined(separator: ", ")).font(.callout).foregroundStyle(Brand.muted)
                    }
                }

                if project.verdict == .active && !project.pinned {
                    Toggle("Clean anyway: this project is in use", isOn: $allowActive)
                        .accessibilityIdentifier("allowActive")
                }

                if project.items.isEmpty {
                    Text("Nothing to clean here.").foregroundStyle(Brand.muted)
                } else {
                    let biggest = project.items.compactMap(\.bytes).max() ?? 1
                    VStack(spacing: 0) {
                        ForEach(Array(project.items.enumerated()), id: \.element.path) { i, item in
                            itemRow(item, share: Double(item.bytes ?? 0) / Double(max(biggest, 1)))
                            if i < project.items.count - 1 { Divider().overlay(Brand.hairline).padding(.leading, 76) }
                        }
                    }
                    .glassCard()
                }
            }
            .padding(.horizontal, 28).padding(.top, 26).padding(.bottom, 90)
        }
        .overlay(alignment: .bottom) {
            HStack(spacing: 14) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(bytes(Selection.bytes(of: picked, in: project))).font(.figure(20)).foregroundStyle(Brand.text)
                    Text(picked.count == 1 ? "1 folder selected" : "\(picked.count) folders selected").font(.caption).foregroundStyle(Brand.muted)
                }
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("selection.summary")
                Spacer()
                Button("Review…") {
                    Task {
                        busy = true
                        review = await model.review(picked, in: project)
                        busy = false
                    }
                }
                .buttonStyle(PrimaryButtonStyle())
                .keyboardShortcut(.defaultAction)
                .disabled(picked.isEmpty || busy || (project.verdict == .active && !allowActive))
                .accessibilityIdentifier("review")
            }
            .padding(.horizontal, 18).padding(.vertical, 12)
            .floatingBar(in: RoundedRectangle(cornerRadius: 16, style: .continuous))
            .padding(.horizontal, 24).padding(.bottom, 18)
        }
        .onAppear { picked = Selection.defaults(for: project) }
        .onChange(of: project) { _, fresh in
            // Drop ticks on folders that are gone or can no longer be cleaned (pinned).
            let still = Set(fresh.items.filter(Selection.selectable).map { Selection.path($0, in: fresh) })
            picked.formIntersection(still)
        }
        .sheet(item: $review) { out in
            ReviewSheet(project: project, picked: picked, dryRun: out)
        }
    }

    private func itemRow(_ item: Item, share: Double) -> some View {
        let path = Selection.path(item, in: project)
        let on = Binding(
            get: { picked.contains(path) },
            set: { if $0 { picked.insert(path) } else { picked.remove(path) } })
        return HStack(alignment: .top, spacing: 12) {
            Toggle("", isOn: on).labelsHidden().toggleStyle(.checkbox)
                .disabled(!Selection.selectable(item) || project.pinned)
                .accessibilityIdentifier("item.\(item.path)")
                .accessibilityLabel("\(item.path), \(item.kind.label), \(item.bytes.map(bytes) ?? "size unknown")")
                .accessibilityHint(item.refused.map { "Kept: \($0)" } ?? item.kind.explanation)
            .padding(.top, 5)
            EcosystemBadge(ecosystem: Ecosystem.of(rule: item.rule), size: 26)
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 8) {
                    Text(item.path).font(.body.monospaced().weight(.semibold)).foregroundStyle(Brand.text)
                    StatusChip(text: item.kind.label, systemImage: item.kind == .safe ? "checkmark.shield.fill" : "exclamationmark.triangle.fill", tint: item.kind.tint)
                }
                Text(item.refused.map { "Kept: \($0)" } ?? item.kind.explanation)
                    .font(.callout).foregroundStyle(Brand.muted)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 12)
            VStack(alignment: .trailing, spacing: 6) {
                Text(item.bytes.map(bytes) ?? "—").font(.callout.weight(.semibold)).monospacedDigit().foregroundStyle(Brand.text)
                ShareBar(share: share, tint: Ecosystem.of(rule: item.rule).color).frame(width: 72)
            }
        }
        .padding(.horizontal, 16).padding(.vertical, 13)
        .opacity(item.refused == nil ? 1 : 0.6)
    }
}

extension CleanOutput: @retroactive Identifiable {
    public var id: String { run?.runId ?? "dry-\(folders)-\(bytes)-\(projects.first?.items.count ?? 0)" }
}

/// The dry run's answer, then the real clean of exactly the same folders.
struct ReviewSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let project: AgentEngine.Project
    let picked: Set<String>
    let dryRun: CleanOutput
    @State private var done: CleanOutput?
    @State private var busy = false

    private var shown: CleanOutput { done ?? dryRun }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text(done == nil ? "Move to quarantine?" : "Moved to quarantine")
                .font(.title2.weight(.semibold))
            Text(done == nil
                 ? "\(shown.folders) folders, \(bytes(shown.bytes)). They stay restorable until you purge them in History; space comes back when you purge."
                 : "\(done?.run?.entries.filter { $0.state == .moved }.count ?? 0) folders moved. Undo from here or from History.")
                .foregroundStyle(Brand.muted)

            List {
                ForEach(shown.projects.flatMap(\.items), id: \.path) { item in
                    HStack {
                        Image(systemName: item.action == .move ? "arrow.right.circle.fill" : "minus.circle")
                            .foregroundStyle(item.action == .move ? Brand.teal : Brand.slate)
                            .accessibilityLabel(item.action == .move ? "Will move" : "Kept")
                        Text(item.path).font(.body.monospaced())
                        Text(item.kind.label).font(.caption).foregroundStyle(item.kind.tint)
                        Spacer()
                        Text(item.bytes.map(bytes) ?? (item.reason ?? "kept")).foregroundStyle(Brand.muted)
                    }
                }
                if let entries = done?.run?.entries.filter({ $0.state != .moved }), !entries.isEmpty {
                    ForEach(entries, id: \.stored) { e in
                        Text("\(URL(fileURLWithPath: e.original).lastPathComponent): \(e.state.rawValue), \(e.note ?? "")")
                            .foregroundStyle(Brand.amber)
                    }
                }
            }
            .frame(minHeight: 160)

            HStack {
                if let run = done?.run {
                    Button("Undo") {
                        Task { await model.restore(run); dismiss() }
                    }
                    .buttonStyle(SecondaryButtonStyle())
                    .accessibilityIdentifier("undo")
                }
                Spacer()
                if done == nil {
                    Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                    Button("Move \(shown.folders) folders") {
                        Task {
                            busy = true
                            done = await model.clean(picked, in: project, execute: true)
                            busy = false
                        }
                    }
                    .buttonStyle(PrimaryButtonStyle())
                    .disabled(shown.folders == 0 || busy)
                    .accessibilityIdentifier("confirmClean")
                } else {
                    Button("Done") { dismiss() }.keyboardShortcut(.defaultAction)
                        .accessibilityIdentifier("done")
                }
            }
        }
        .padding(24)
        .frame(width: 560, height: 460)
    }
}

/// A list row in the Projects column: narrower than ProjectRow.
struct CompactProjectRow: View {
    let project: AgentEngine.Project
    let total: UInt64
    var share: Double = 0

    var body: some View {
        HStack(spacing: 12) {
            EcosystemStack(ecosystems: project.ecosystems.isEmpty ? [Ecosystem.of(rule: "")] : project.ecosystems, size: 24)
            VStack(alignment: .leading, spacing: 3) {
                Text(URL(fileURLWithPath: project.root).lastPathComponent).font(.body.weight(.semibold)).foregroundStyle(Brand.text)
                    .lineLimit(1)
                HStack(spacing: 5) {
                    Image(systemName: project.pinned ? "pin.fill" : project.verdict.symbol).font(.system(size: 9, weight: .bold))
                        .accessibilityHidden(true)
                    Text(project.pinned ? "Pinned" : project.verdict.label)
                }
                .font(.caption.weight(.medium))
                .foregroundStyle(project.pinned ? Brand.slate : project.verdict.tint)
            }
            Spacer(minLength: 8)
            VStack(alignment: .trailing, spacing: 5) {
                Text(total == 0 ? "—" : bytes(total)).font(.callout.weight(.semibold)).monospacedDigit().foregroundStyle(Brand.text)
                ShareBar(share: share, tint: project.ecosystems.first?.color ?? Brand.teal).frame(width: 56)
            }
        }
        .padding(.horizontal, 10).padding(.vertical, 9)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }
}
