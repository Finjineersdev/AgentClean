// SPDX-License-Identifier: Apache-2.0

import XCTest
@testable import AgentEngine

private let repo = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent().deletingLastPathComponent()
    .deletingLastPathComponent().deletingLastPathComponent()

/// The engine's own JSON snapshots (one array of output lines per file).
private func snapshot(_ name: String) throws -> [Data] {
    let url = repo.appendingPathComponent("engine/crates/agentclean-cli/tests/snapshots/\(name).json")
    let lines = try JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [Any] ?? []
    return try lines.map { try JSONSerialization.data(withJSONObject: $0) }
}

private func one<T: Decodable>(_ type: T.Type, _ name: String) throws -> T {
    let lines = try snapshot(name)
    XCTAssertEqual(lines.count, 1, name)
    return try Engine.decode(type, from: lines[0])
}

/// Every shape the engine documents decodes from the engine's snapshots.
final class EngineContractTests: XCTestCase {
    func testScanLines() throws {
        let events = try snapshot("scan").map { try Engine.decode(ScanEvent.self, from: $0) }
        guard case .project(let p) = events.first, case .summary(let s) = events.last else {
            return XCTFail("\(events)")
        }
        XCTAssertEqual(p.root, "$TMP/work/site")
        XCTAssertEqual(p.verdict, .dormant)
        XCTAssertEqual(p.items.map(\.path), ["node_modules", ".next"])
        XCTAssertEqual(p.items.map(\.kind), [.caution, .safe])
        XCTAssertEqual(s.projects, 1)
        XCTAssertEqual(s.ready.folders, 1)
    }

    func testSkippedLine() throws {
        let line = Data(#"{"schema_version":1,"type":"skipped","path":"/x","reason":"other_volume","detail":"on another volume"}"#.utf8)
        XCTAssertEqual(
            try Engine.decode(ScanEvent.self, from: line),
            .skipped(path: "/x", reason: .otherVolume, detail: "on another volume"))
    }

    func testPlanCleanAndRuns() throws {
        XCTAssertEqual(try one(PlanOutput.self, "plan").project.items.count, 2)
        let dry = try one(CleanOutput.self, "clean-dry-run")
        XCTAssertFalse(dry.execute)
        XCTAssertNil(dry.run)
        let done = try one(CleanOutput.self, "clean-execute")
        XCTAssertEqual(done.projects[0].items.map(\.action), [.move, .move])
        XCTAssertEqual(done.run?.entries.map(\.state), [.moved, .moved])
        XCTAssertEqual(done.run?.entries[0].projectRoot, "$TMP/work/site")
        XCTAssertEqual(try one(HistoryOutput.self, "history").runs.count, 1)
        XCTAssertEqual(try one(RestoreOutput.self, "restore").run.entries[0].state, .restored)
        XCTAssertFalse(try one(PurgeOutput.self, "purge-dry-run").execute)
        XCTAssertEqual(try one(PinsOutput.self, "pin").pinned, ["$TMP/work/site"])
        XCTAssertEqual(try one(ConfigOutput.self, "config").idleDays, 7)
        let caches = try one(CachesOutput.self, "caches").caches
        XCTAssertEqual(caches.map(\.id), ["npm"])
        XCTAssertTrue(caches[0].clearable)
        XCTAssertFalse(try one(CleanCacheOutput.self, "clean-cache-dry-run").execute)
        XCTAssertEqual(try one(CleanCacheOutput.self, "clean-cache-execute").caches[0].status, 0)
        XCTAssertEqual(try one(ErrorOutput.self, "error").error, #"no run named "nope""#)
    }
}

/// Runs the real binary on a throwaway project. Set AGENTCLEAN_BIN, or
/// build engine/target/debug/agentclean first; skipped otherwise.
final class EngineEndToEndTests: XCTestCase {
    private var tmp: URL!
    private var engine: Engine!
    private var project: URL!

    override func setUpWithError() throws {
        let env = ProcessInfo.processInfo.environment
        let binary = env["AGENTCLEAN_BIN"].map(URL.init(fileURLWithPath:))
            ?? repo.appendingPathComponent("engine/target/debug/agentclean")
        try XCTSkipUnless(FileManager.default.isExecutableFile(atPath: binary.path),
                          "no engine binary at \(binary.path)")
        let fm = FileManager.default
        tmp = fm.temporaryDirectory.resolvingSymlinksInPath().appendingPathComponent(UUID().uuidString)
        let home = tmp.appendingPathComponent("home")
        project = tmp.appendingPathComponent("work/site")
        try fm.createDirectory(at: project.appendingPathComponent(".next/cache"), withIntermediateDirectories: true)
        try fm.createDirectory(at: project.appendingPathComponent("node_modules/a"), withIntermediateDirectories: true)
        try fm.createDirectory(at: home, withIntermediateDirectories: true)
        try Data(#"{"dependencies":{"next":"1"}}"#.utf8).write(to: project.appendingPathComponent("package.json"))
        try Data("x".utf8).write(to: project.appendingPathComponent(".next/cache/a"))
        try Data("x".utf8).write(to: project.appendingPathComponent("node_modules/a/i.js"))
        // Old enough to be Dormant.
        let old = Date(timeIntervalSinceNow: -200 * 86_400)
        for path in fm.enumerator(at: project, includingPropertiesForKeys: nil)!.compactMap({ $0 as? URL }) + [project!] {
            try fm.setAttributes([.modificationDate: old], ofItemAtPath: path.path)
        }
        engine = Engine(
            binary: binary,
            store: tmp.appendingPathComponent("store"),
            environment: ["HOME": home.path, "XDG_CONFIG_HOME": "", "XDG_DATA_HOME": ""])
    }

    override func tearDownWithError() throws {
        if let tmp { try? FileManager.default.removeItem(at: tmp) }
    }

    func testScanStreamsProjectsThenSummary() async throws {
        var events: [ScanEvent] = []
        for try await event in engine.scan(roots: [tmp.appendingPathComponent("work")]) {
            events.append(event)
        }
        guard case .project(let p) = events.first, case .summary(let s) = events.last else {
            return XCTFail("\(events)")
        }
        // Foundation drops /private when resolving; the engine keeps it.
        XCTAssertEqual(URL(fileURLWithPath: p.root).resolvingSymlinksInPath(), project.resolvingSymlinksInPath())
        XCTAssertEqual(s.projects, 1)
    }

    func testCleanOnlyThenRestore() async throws {
        let next = project.appendingPathComponent(".next")
        let dry = try await engine.clean(projects: [project], only: [next])
        XCTAssertEqual(dry.folders, 1)
        XCTAssertTrue(FileManager.default.fileExists(atPath: next.path), "a dry run moves nothing")

        let done = try await engine.clean(projects: [project], only: [next], execute: true)
        let run = try XCTUnwrap(done.run)
        XCTAssertFalse(FileManager.default.fileExists(atPath: next.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: project.appendingPathComponent("node_modules").path))

        let runs = try await engine.history()
        XCTAssertEqual(runs.map(\.runId), [run.runId])
        let back = try await engine.restore(run: run.runId)
        XCTAssertEqual(back.entries.map(\.state), [.restored])
        XCTAssertTrue(FileManager.default.fileExists(atPath: next.appendingPathComponent("cache/a").path))
    }

    func testEngineErrorsSurface() async throws {
        do {
            _ = try await engine.restore(run: "nope")
            XCTFail("expected an error")
        } catch EngineError.failed(let message) {
            XCTAssertTrue(message.contains("no run named"), message)
        }
        do {
            _ = try await engine.clean(projects: [project], only: [project.appendingPathComponent("src")])
            XCTFail("expected an error")
        } catch EngineError.failed(let message) {
            XCTAssertTrue(message.contains("No such file") || message.contains("not a folder"), message)
        }
    }
}

/// F14: Caution is never pre-selected; active and pinned projects start empty.
final class SelectionTests: XCTestCase {
    private func project(_ verdict: Verdict, pinned: Bool = false) -> Project {
        Project(root: "/w/site", verdict: verdict, pinned: pinned, reason: "", items: [
            Item(path: ".next", rule: "node", kind: .safe, bytes: 10, refused: nil),
            Item(path: "node_modules", rule: "node", kind: .caution, bytes: 20, refused: nil),
            Item(path: "dist", rule: "node", kind: .safe, bytes: nil, refused: "not a directory"),
        ])
    }

    func testDefaultsAreSafeAllowedFoldersOnly() {
        XCTAssertEqual(Selection.defaults(for: project(.dormant)), ["/w/site/.next"])
        XCTAssertEqual(Selection.defaults(for: project(.idle)), ["/w/site/.next"])
    }

    func testActiveAndPinnedProjectsStartEmpty() {
        XCTAssertEqual(Selection.defaults(for: project(.active)), [])
        XCTAssertEqual(Selection.defaults(for: project(.dormant, pinned: true)), [])
    }

    func testRefusedItemsAreNotSelectable() {
        let p = project(.idle)
        XCTAssertEqual(p.items.map(Selection.selectable), [true, true, false])
    }

    func testFlagsFollowThePicks() {
        let p = project(.idle)
        XCTAssertFalse(Selection.flags(for: ["/w/site/.next"], in: p).caution)
        XCTAssertTrue(Selection.flags(for: ["/w/site/node_modules"], in: p).caution)
        XCTAssertTrue(Selection.flags(for: [], in: project(.active)).active)
        XCTAssertEqual(Selection.bytes(of: ["/w/site/.next", "/w/site/node_modules"], in: p), 30)
    }
}
