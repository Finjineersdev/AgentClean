#if DEBUG
import AppKit
import SwiftUI
import AgentEngine

/// Debug builds only. With AGENTCLEAN_QA=<folder>, the app walks through
/// every screen against whatever the scan finds, snapshots its own window
/// after each step, checks the outcome on disk, writes report.txt and quits.
/// Run it against throwaway projects only (point HOME at a temp folder).
@MainActor
enum QAWalkthrough {
    static var folder: URL? {
        ProcessInfo.processInfo.environment["AGENTCLEAN_QA"].map(URL.init(fileURLWithPath:))
    }

    static func run(_ model: AppModel) async {
        guard let out = folder else { return }
        var report: [String] = []
        func check(_ ok: Bool, _ what: String) { report.append("\(ok ? "PASS" : "FAIL")  \(what)") }
        func pause(_ s: Double = 1.2) async { try? await Task.sleep(for: .seconds(s)) }
        func snap(_ name: String) {
            // An app may capture its own windows without Screen Recording
            // permission. CGWindowListCreateImage is obsolete in the SDK, so
            // it is looked up at run time; debug builds only.
            typealias Capture = @convention(c) (CGRect, UInt32, UInt32, UInt32) -> Unmanaged<CGImage>?
            guard let window = NSApp.windows.first(where: { $0.isVisible && $0.isSheet })
                    ?? NSApp.windows.first(where: { $0.isVisible && $0.title != "Settings" }),
                  let sym = dlsym(UnsafeMutableRawPointer(bitPattern: -2), "CGWindowListCreateImage") else { return }
            let capture = unsafeBitCast(sym, to: Capture.self)
            // .null bounds = the window's own frame; option 8 = including window;
            // image option 1 = ignore framing.
            guard let image = capture(.null, 8, UInt32(window.windowNumber), 1)?.takeRetainedValue() else { return }
            try? NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:])?
                .write(to: out.appendingPathComponent("\(name).png"))
        }
        let fm = FileManager.default
        try? fm.createDirectory(at: out, withIntermediateDirectories: true)

        await pause(2)
        check(model.engine != nil, "the bundled engine was found")
        snap("01-overview")

        model.toggleScan()
        while model.scanning { await pause(0.3) }
        await pause()
        check(model.summary != nil, "scan finished with a summary (\(model.projects.count) projects)")
        check(model.error == nil, "no error after scan: \(model.error ?? "")")
        snap("02-overview-scanned")

        model.screen = .projects
        let shop = model.projects.first { $0.root.hasSuffix("/shop-web") }
        model.selectedProject = shop?.root
        await pause()
        snap("03-projects-shop-web")
        if let shop {
            let picked = Selection.defaults(for: shop)
            check(picked.count == 1 && picked.first?.hasSuffix("/.next") == true, "only Safe .next is pre-selected")
            let dry = await model.review(picked, in: shop)
            check(dry?.execute == false && dry?.folders == 1, "review is a one-folder dry run")
            check(fm.fileExists(atPath: shop.root + "/.next"), ".next still there after review")
            let done = await model.clean(picked, in: shop, execute: true)
            check(done?.run?.entries.map(\.state) == [.moved], "clean moved exactly one folder")
            check(!fm.fileExists(atPath: shop.root + "/.next"), ".next is gone from the project")
            check(fm.fileExists(atPath: shop.root + "/node_modules"), "node_modules (Caution) untouched")
            await pause()
            snap("04-projects-after-clean")
        } else {
            check(false, "shop-web was found")
        }

        let live = model.projects.first { $0.root.hasSuffix("/live-api") }
        model.selectedProject = live?.root
        await pause()
        check(live?.verdict == .active, "live-api is Active")
        check(live.map { Selection.defaults(for: $0).isEmpty } ?? false, "nothing pre-selected in an active project")
        snap("05-projects-active")

        let pinned = model.projects.first { $0.root.hasSuffix("/keep-me") }
        model.selectedProject = pinned?.root
        await pause()
        check(pinned?.pinned == true && pinned?.items.allSatisfy { !Selection.selectable($0) } == true, "pinned project offers nothing")
        snap("06-projects-pinned")

        model.screen = .history
        await model.loadHistory()
        await pause()
        check(model.runs.count == 1, "history lists the run")
        snap("07-history")
        if let run = model.runs.first {
            let preview = await model.purgePreview(run)
            check((preview ?? 0) > 0, "purge preview reports bytes without deleting")
            await model.restore(run)
            check(shop.map { fm.fileExists(atPath: $0.root + "/.next") } ?? false, "restore put .next back")
            await pause()
            snap("08-history-restored")
        }

        // Review all ready: one run across projects, one Undo (gap review P1).
        let plan = model.readyPlan()
        check(plan.count >= 2 && !plan.contains { $0.project.verdict == .active || $0.project.pinned },
              "review all ready spans \(plan.count) projects, none active or pinned")
        let before = model.summary?.ready.bytes ?? 0
        let bulk = await model.clean(plan, execute: true)
        let cleanedRoots = Set(bulk?.run?.entries.filter { $0.state == .moved }.map(\.projectRoot) ?? [])
        check(cleanedRoots.count == plan.count, "one run cleaned every ready project")
        check((model.summary?.ready.bytes ?? 0) < before, "totals drop after a bulk clean")
        if let run = bulk?.run { await model.restore(run) }
        check(model.summary?.ready.bytes == before, "one Undo restores them all")

        // The disk map sits below the fold: render it on its own.
        let map = ImageRenderer(content: DiskMap(stats: LanguageStat.of(model.projects))
            .environment(model).padding(24).frame(width: 980).background(Brand.canvas))
        map.scale = 2
        if let cg = map.cgImage {
            try? NSBitmapImageRep(cgImage: cg).representation(using: .png, properties: [:])?
                .write(to: out.appendingPathComponent("12-disk-map.png"))
        }
        check(map.cgImage != nil, "the disk map renders")

        // Scheduled clean (P5): dormant projects' Safe folders only, one run, one Undo.
        let sched = await model.runSchedule(cap: 1_000_000_000)
        let schedRoots = Set(sched?.entries.map(\.projectRoot) ?? [])
        check(sched != nil && !schedRoots.contains { $0.hasSuffix("/live-api") || $0.hasSuffix("/keep-me") }
              && sched?.entries.allSatisfy { $0.kind == .safe } == true,
              "scheduled clean took \(sched?.entries.count ?? 0) Safe folders, none active or pinned")
        check(fm.fileExists(atPath: shop.map { $0.root + "/node_modules" } ?? ""), "scheduled clean left node_modules")
        if let sched { await model.undoRun(sched.runId) }
        check(shop.map { fm.fileExists(atPath: $0.root + "/.next") } ?? false, "Undo from the notification restores it")

        // Global caches: needs ~/.npm/_cacache and a stand-in npm on PATH
        // (see QA-CHECKLIST.md); never the real npm.
        model.screen = .caches
        await model.loadCaches()
        await pause()
        let npm = model.caches.first { $0.id == "npm" }
        check(npm?.clearable == true, "the npm cache is listed and clearable")
        check(model.caches.contains { $0.id == "gradle" && !$0.clearable }, "Gradle is measured only")
        check(model.caches.contains { $0.id == "huggingface" && !$0.clearable }, "Hugging Face models are measured only")
        snap("09-caches")
        if let npm {
            let done = await model.clearCaches([npm.id])
            check(done.first?.status == 0, "npm's own command ran")
            check(!fm.fileExists(atPath: npm.path), "the npm cache is gone")
            check(!model.caches.contains { $0.id == "npm" }, "the list refreshed")
        }

        model.screen = .devStack
        model.loadDevStack()
        while model.devStackLoading { await pause(0.5) }
        await pause()
        check(!model.devStack.isEmpty, "dev stack inventory loaded (\(model.devStack.count) items)")
        snap("10-dev-stack")

        model.onboarding = true
        await pause(1.5)
        check(NSApp.windows.contains { $0.isVisible && $0.isSheet }, "the first-run guide opens")
        snap("11-onboarding")
        model.onboarding = false
        await pause(0.6)

        await model.setDays(idle: 7, dormant: 60)
        check(model.config?.idleDays == 7, "settings save through the engine")

        try? report.joined(separator: "\n").appending("\n")
            .write(to: out.appendingPathComponent("report.txt"), atomically: true, encoding: .utf8)
        NSApp.terminate(nil)
    }
}
#endif
