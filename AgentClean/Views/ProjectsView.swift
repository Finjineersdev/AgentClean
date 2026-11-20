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
            VStack(alignment: .leading, spacing: 12) {
                ScreenHeader(title: "Projects", subtitle: "\(model.projects.count) found")
                TextField("Search", text: $search).textFieldStyle(.roundedBorder)
                if model.projects.isEmpty {
                    Spacer()
                    Text(model.scanning ? "Scanning…" : "Run a scan from Overview to find projects.")
                        .foregroundStyle(Brand.muted).frame(maxWidth: .infinity)
                    Spacer()
                } else {
                    ScrollView {
                        LazyVStack(spacing: 6) {
                            ForEach(rows, id: \.root) { p in
                                Button { model.selectedProject = p.root } label: {
                                    ProjectRow(project: p, total: model.total(p))
                                        .overlay(RoundedRectangle(cornerRadius: 12)
                                            .stroke(Brand.teal, lineWidth: model.selectedProject == p.root ? 1.5 : 0))
                                }
                                .buttonStyle(.plain)
                                .accessibilityIdentifier("project.\(URL(fileURLWithPath: p.root).lastPathComponent)")
                            }
                        }
                    }
                }
            }
            .padding(24)
            .frame(width: 400)

            Divider().opacity(0.3)

            if let p = model.project(model.selectedProject) {
                // Keyed by path, not value: a refresh after a clean must not tear down
                // the open Review sheet (and its Undo).
                ProjectDetail(project: p).id(p.root)
            } else {
                Text("Choose a project").foregroundStyle(Brand.muted).frame(maxWidth: .infinity, maxHeight: .infinity)
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
                HStack(alignment: .top) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(URL(fileURLWithPath: project.root).lastPathComponent)
                            .font(.system(size: 26, weight: .semibold, design: .rounded))
                        Text((project.root as NSString).abbreviatingWithTildeInPath).foregroundStyle(Brand.muted).textSelection(.enabled)
                    }
                    Spacer()
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
                }

                if project.verdict == .active && !project.pinned {
                    Toggle("Clean anyway: this project is in use", isOn: $allowActive)
                        .accessibilityIdentifier("allowActive")
                }

                VStack(spacing: 6) {
                    ForEach(project.items, id: \.path) { item in
                        itemRow(item)
                    }
                    if project.items.isEmpty {
                        Text("Nothing to clean here.").foregroundStyle(Brand.muted)
                    }
                }

                HStack {
                    Text("\(picked.count) selected · \(bytes(Selection.bytes(of: picked, in: project)))")
                        .foregroundStyle(Brand.muted).monospacedDigit()
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
                    .disabled(picked.isEmpty || busy || (project.verdict == .active && !allowActive))
                    .accessibilityIdentifier("review")
                }
            }
            .padding(24)
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

    private func itemRow(_ item: Item) -> some View {
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
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 8) {
                    Text(item.path).font(.body.monospaced().weight(.semibold))
                    Text(item.rule).font(.caption).foregroundStyle(Brand.muted)
                    Text(item.kind.label).font(.caption.weight(.semibold)).foregroundStyle(item.kind.tint)
                }
                Text(item.refused.map { "Kept: \($0)" } ?? item.kind.explanation)
                    .font(.caption).foregroundStyle(Brand.muted)
            }
            Spacer()
            Text(item.bytes.map(bytes) ?? "—").monospacedDigit()
        }
        .padding(12)
        .glassCard(radius: 12)
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
