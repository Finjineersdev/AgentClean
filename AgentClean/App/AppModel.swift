import Foundation
import Observation
import CleanCore
import AgentEngine

/// App state. Every change to files goes through `engine` (ADR 0002, 0005);
/// nothing here moves or deletes anything itself.
@MainActor
@Observable
final class AppModel {
    enum Screen: String, CaseIterable, Identifiable {
        case overview, projects, caches, history, devStack
        var id: String { rawValue }
        var title: String {
            switch self {
            case .overview: "Overview"
            case .projects: "Projects"
            case .caches: "Caches"
            case .history: "History"
            case .devStack: "Dev Stack"
            }
        }
        var symbol: String {
            switch self {
            case .overview: "sparkles"
            case .projects: "folder"
            case .caches: "internaldrive"
            case .history: "clock.arrow.circlepath"
            case .devStack: "shippingbox"
            }
        }
    }

    var screen: Screen = .overview
    var selectedProject: String?

    // Scan
    private(set) var projects: [Project] = []
    private(set) var skipped: [(path: String, detail: String)] = []
    private(set) var summary: ScanSummary?
    private(set) var scanning = false
    private(set) var lastScan: Date?
    private var scanTask: Task<Void, Never>?

    // History, settings, inventory
    private(set) var runs: [Run] = []
    private(set) var config: ConfigOutput?
    private(set) var devStack: [DevStackItem] = []
    private(set) var devStackLoading = false
    private(set) var caches: [ToolCache] = []
    private(set) var cachesLoading = false

    var error: String?

    let engine: Engine?

    /// Folders the scan starts from (the app's own preference).
    var scanRoots: [String] {
        didSet { UserDefaults.standard.set(scanRoots, forKey: "scanRoots") }
    }

    init() {
        engine = Self.locateEngine().map { Engine(binary: $0) }
        scanRoots = UserDefaults.standard.stringArray(forKey: "scanRoots") ?? [CMConstants.homePath]
        if engine == nil { error = "The engine is missing from the app bundle. Reinstall AgentClean." }
    }

    /// The bundled binary, or AGENTCLEAN_BIN when developing.
    static func locateEngine() -> URL? {
        if let dev = ProcessInfo.processInfo.environment["AGENTCLEAN_BIN"] {
            return URL(fileURLWithPath: dev)
        }
        // Contents/Helpers: Contents/MacOS ignores case and holds AgentClean.
        let helper = Bundle.main.bundleURL.appendingPathComponent("Contents/Helpers/agentclean")
        return FileManager.default.isExecutableFile(atPath: helper.path) ? helper : nil
    }

    // MARK: Scan

    var cleanable: [Project] {
        projects.filter { p in p.items.contains(where: Selection.selectable) }
            .sorted { total($0) > total($1) }
    }

    func total(_ p: Project) -> UInt64 { p.items.reduce(0) { $0 + ($1.bytes ?? 0) } }

    func project(_ root: String?) -> Project? { projects.first { $0.root == root } }

    func toggleScan() {
        if scanning { scanTask?.cancel(); return }
        guard let engine else { return }
        scanning = true
        projects = []
        skipped = []
        summary = nil
        let roots = scanRoots.map { URL(fileURLWithPath: $0) }
        scanTask = Task {
            do {
                for try await event in engine.scan(roots: roots) {
                    switch event {
                    case .project(let p): projects.append(p)
                    case .skipped(let path, _, let detail): skipped.append((path, detail))
                    case .summary(let s): summary = s
                    }
                }
                lastScan = Date()
            } catch is CancellationError {
            } catch {
                if !Task.isCancelled { self.error = describe(error) }
            }
            scanning = false
        }
    }

    /// Re-reads one project after a clean, restore or pin.
    func refresh(_ root: String) async {
        guard let engine else { return }
        do {
            let fresh = try await engine.plan(project: URL(fileURLWithPath: root)).project
            if let i = projects.firstIndex(where: { $0.root == root }) { projects[i] = fresh }
        } catch {
            projects.removeAll { $0.root == root }
        }
    }

    // MARK: Clean

    /// A dry run of exactly `picked` in `project`.
    func review(_ picked: Set<String>, in project: Project) async -> CleanOutput? {
        await clean(picked, in: project, execute: false)
    }

    func clean(_ picked: Set<String>, in project: Project, execute: Bool) async -> CleanOutput? {
        guard let engine, !picked.isEmpty else { return nil }
        let flags = Selection.flags(for: picked, in: project)
        do {
            let out = try await engine.clean(
                projects: [URL(fileURLWithPath: project.root)],
                only: picked.sorted().map { URL(fileURLWithPath: $0) },
                includeCaution: flags.caution,
                includeActive: flags.active,
                execute: execute)
            if execute {
                await refresh(project.root)
                await loadHistory()
            }
            return out
        } catch {
            self.error = describe(error)
            return nil
        }
    }

    // MARK: History

    func loadHistory() async {
        guard let engine else { return }
        do { runs = try await engine.history() } catch { self.error = describe(error) }
    }

    func restore(_ run: Run, item: Int? = nil) async {
        guard let engine else { return }
        do {
            let back = try await engine.restore(run: run.runId, item: item)
            await loadHistory()
            for root in Set(back.entries.map(\.projectRoot)) { await refresh(root) }
            if let stuck = back.entries.first(where: { $0.state == .moved && $0.note != nil }) {
                error = "Some folders stayed in quarantine: \(stuck.note ?? "")"
            }
        } catch {
            self.error = describe(error)
        }
    }

    /// What purging would free, without deleting.
    func purgePreview(_ run: Run) async -> UInt64? {
        guard let engine else { return nil }
        return try? await engine.purge(run: run.runId).bytes
    }

    func purge(_ run: Run) async {
        guard let engine else { return }
        do {
            _ = try await engine.purge(run: run.runId, execute: true)
            await loadHistory()
        } catch {
            self.error = describe(error)
        }
    }

    // MARK: Settings

    func loadConfig() async {
        guard let engine else { return }
        do { config = try await engine.config() } catch { self.error = describe(error) }
    }

    func setDays(idle: Int, dormant: Int) async {
        guard let engine else { return }
        do { config = try await engine.config(idleDays: idle, dormantDays: dormant) } catch { self.error = describe(error) }
    }

    func exclude(_ folder: URL) async {
        guard let engine else { return }
        do { config = try await engine.config(exclude: [folder]) } catch { self.error = describe(error) }
    }

    func include(_ folder: String) async {
        guard let engine else { return }
        do { config = try await engine.config(include: [URL(fileURLWithPath: folder)]) } catch { self.error = describe(error) }
    }

    func setPinned(_ root: String, _ pinned: Bool) async {
        guard let engine else { return }
        do {
            let url = URL(fileURLWithPath: root)
            _ = pinned ? try await engine.pin(url) : try await engine.unpin(url)
            await loadConfig()
            await refresh(root)
        } catch {
            self.error = describe(error)
        }
    }

    // MARK: Global caches (ADR 0006)

    func loadCaches() async {
        guard let engine, !cachesLoading else { return }
        cachesLoading = true
        defer { cachesLoading = false }
        do { caches = try await engine.caches() } catch { self.error = describe(error) }
    }

    /// Runs each tool's own command. Not undoable; the caller has asked first.
    func clearCaches(_ ids: [String]) async -> [ToolCache] {
        guard let engine, !ids.isEmpty else { return [] }
        do {
            let out = try await engine.cleanCaches(ids, execute: true)
            await loadCaches()
            return out.caches
        } catch {
            self.error = describe(error)
            return []
        }
    }

    // MARK: Dev Stack (read-only)

    func loadDevStack() {
        guard !devStackLoading else { return }
        devStackLoading = true
        Task {
            devStack = await DevStackInventory.collect()
            devStackLoading = false
        }
    }

    private func describe(_ error: Error) -> String {
        switch error {
        case EngineError.failed(let m): m
        case EngineError.badOutput(let m): "Unexpected output from the engine: \(m)"
        case EngineError.launch(let m): "The engine could not start: \(m)"
        default: error.localizedDescription
        }
    }
}
