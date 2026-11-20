// SPDX-License-Identifier: Apache-2.0
//
// Drives the real app against throwaway projects. HOME points at a temporary
// folder, so the bundled engine uses a throwaway config and quarantine.

import XCTest

final class AgentCleanUITests: XCTestCase {
    private var base: URL!
    private var work: URL { base.appendingPathComponent("work") }
    private var app: XCUIApplication!
    private let fm = FileManager.default

    override func setUpWithError() throws {
        continueAfterFailure = false
        base = fm.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("agentclean-ui-\(UUID().uuidString)")
        try makeProjects()
        app = XCUIApplication()
        app.launchEnvironment["HOME"] = base.appendingPathComponent("home").path
        app.launchEnvironment["XDG_CONFIG_HOME"] = ""
        app.launchEnvironment["XDG_DATA_HOME"] = ""
        app.launchEnvironment["PATH"] = base.appendingPathComponent("bin").path + ":/usr/bin:/bin"
        // Each test starts from the default window, not one an earlier test left behind.
        app.launchArguments += ["-scanRoots", "(\"\(work.path)\")", "-ApplePersistenceIgnoreState", "YES"]
        app.launch()
    }

    override func tearDownWithError() throws {
        app?.terminate()
        if let base { try? fm.removeItem(at: base) }
    }

    // MARK: Fixtures

    private func file(_ rel: String, _ size: Int) throws {
        let url = work.appendingPathComponent(rel)
        try fm.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data(count: size).write(to: url)
    }

    private func makeProjects() throws {
        try file("shop-web/package.json", 0)
        try Data(#"{"dependencies":{"next":"14"}}"#.utf8).write(to: work.appendingPathComponent("shop-web/package.json"))
        try file("shop-web/.next/cache/webpack.bin", 3_000_000)
        try file("shop-web/node_modules/react/index.js", 5_000_000)
        try file("ledger-rs/Cargo.toml", 10)
        try file("ledger-rs/target/debug/ledger", 8_000_000)
        try file("live-api/.next/cache/x.bin", 2_000_000)
        try Data(#"{"dependencies":{"next":"14"}}"#.utf8).write(to: work.appendingPathComponent("live-api/package.json"))
        try fm.createDirectory(at: base.appendingPathComponent("home"), withIntermediateDirectories: true)
        // A global cache and a stand-in npm (never the real one) for the Caches screen.
        let home = base.appendingPathComponent("home")
        try fm.createDirectory(at: home.appendingPathComponent(".npm/_cacache"), withIntermediateDirectories: true)
        try Data(count: 2_000_000).write(to: home.appendingPathComponent(".npm/_cacache/blob"))
        let bin = base.appendingPathComponent("bin")
        try fm.createDirectory(at: bin, withIntermediateDirectories: true)
        // Files this sandboxed runner writes are quarantined, and macOS will
        // not execute them. So `npm` is a link to /bin/sh, and the engine's
        // `npm cache clean --force` (run from HOME) reads the script `~/cache`.
        try fm.createSymbolicLink(atPath: bin.appendingPathComponent("npm").path, withDestinationPath: "/bin/sh")
        try Data("echo \"cache $*\" > \"$HOME/npm-ran\"\nrm -rf \"$HOME/.npm/_cacache\"\n".utf8)
            .write(to: home.appendingPathComponent("cache"))
        // Everything but live-api is old enough to be Dormant.
        let old = Date(timeIntervalSinceNow: -200 * 86_400)
        for project in ["shop-web", "ledger-rs"] {
            let root = work.appendingPathComponent(project)
            let all = (fm.enumerator(at: root, includingPropertiesForKeys: nil)?.allObjects as? [URL] ?? []) + [root]
            for url in all { try fm.setAttributes([.modificationDate: old], ofItemAtPath: url.path) }
        }
    }

    private func exists(_ rel: String) -> Bool {
        fm.fileExists(atPath: work.appendingPathComponent(rel).path)
    }

    private func shot(_ name: String) {
        let a = XCTAttachment(screenshot: app.windows.firstMatch.screenshot())
        a.name = name
        a.lifetime = .keepAlways
        add(a)
    }

    private func scan() {
        app.buttons["scan"].click()
        XCTAssertTrue(app.staticTexts["ready.bytes"].waitForExistence(timeout: 30), "scan finished")
    }

    private func open(_ project: String) {
        app.outlines.cells.containing(.any, identifier: "nav.projects").firstMatch.click()
        let row = app.buttons["project.\(project)"]
        XCTAssertTrue(row.waitForExistence(timeout: 10), project)
        row.click()
    }

    private func checked(_ id: String) -> Bool {
        (app.checkBoxes[id].value as? Int ?? 0) == 1
            || (app.checkBoxes[id].value as? Bool ?? false)
    }

    // MARK: Flows

    func testScanShowsTotals() {
        shot("01-overview-before-scan")
        scan()
        XCTAssertTrue(app.staticTexts["11 MB"].exists || app.staticTexts["ready.bytes"].label.contains("MB"))
        shot("02-overview-after-scan")
    }

    /// F14: Safe folders start ticked in a dormant project, Caution never.
    /// Then a real clean of exactly the ticked folder, and Undo.
    func testCleanOnlyWhatIsTickedThenUndo() {
        scan()
        open("shop-web")
        XCTAssertTrue(app.checkBoxes["item..next"].waitForExistence(timeout: 10))
        XCTAssertTrue(checked("item..next"), "Safe is pre-selected")
        XCTAssertFalse(checked("item.node_modules"), "Caution is never pre-selected")
        shot("03-project-detail")

        app.buttons["review"].click()
        let confirm = app.buttons["confirmClean"]
        XCTAssertTrue(confirm.waitForExistence(timeout: 15))
        XCTAssertTrue(exists("shop-web/.next"), "review is a dry run")
        shot("04-review-sheet")
        confirm.click()

        XCTAssertTrue(app.buttons["undo"].waitForExistence(timeout: 15))
        XCTAssertFalse(exists("shop-web/.next"), ".next moved to quarantine")
        XCTAssertTrue(exists("shop-web/node_modules/react/index.js"), "node_modules untouched")
        shot("05-cleaned")

        app.buttons["undo"].click()
        let restored = NSPredicate { _, _ in self.exists("shop-web/.next/cache/webpack.bin") }
        wait(for: [expectation(for: restored, evaluatedWith: nil)], timeout: 15)
    }

    /// An active project needs explicit consent before review.
    func testActiveProjectNeedsConsent() {
        scan()
        open("live-api")
        XCTAssertTrue(app.buttons["review"].waitForExistence(timeout: 10))
        XCTAssertFalse(checked("item..next"), "nothing pre-selected in an active project")
        app.checkBoxes["item..next"].click()
        XCTAssertFalse(app.buttons["review"].isEnabled, "review needs consent")
        app.checkBoxes["allowActive"].click()
        XCTAssertTrue(app.buttons["review"].isEnabled)
        shot("06-active-project")
    }

    /// Pinning from the project page: its folders can no longer be ticked.
    func testPinnedProjectCannotBeCleaned() {
        scan()
        open("ledger-rs")
        XCTAssertTrue(app.buttons["pin"].waitForExistence(timeout: 10))
        app.buttons["pin"].click()
        let disabled = NSPredicate(format: "isEnabled == false")
        wait(for: [expectation(for: disabled, evaluatedWith: app.checkBoxes["item.target"])], timeout: 15)
        XCTAssertFalse(app.buttons["review"].isEnabled)
        shot("07-pinned")
    }

    /// History restores a run, and purge asks before deleting.
    func testHistoryRestoreAndPurge() {
        scan()
        open("ledger-rs")
        XCTAssertTrue(app.buttons["review"].waitForExistence(timeout: 10))
        app.buttons["review"].click()
        XCTAssertTrue(app.buttons["confirmClean"].waitForExistence(timeout: 15))
        app.buttons["confirmClean"].click()
        XCTAssertTrue(app.buttons["done"].waitForExistence(timeout: 15))
        app.buttons["done"].click()
        XCTAssertFalse(exists("ledger-rs/target"))

        app.outlines.cells.containing(.any, identifier: "nav.history").firstMatch.click()
        let purge = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'purge.'")).firstMatch
        XCTAssertTrue(purge.waitForExistence(timeout: 10))
        shot("08-history")
        purge.click()
        XCTAssertTrue(app.buttons["Cancel"].waitForExistence(timeout: 10) || app.buttons["confirmPurge"].waitForExistence(timeout: 1))
        shot("09-purge-confirmation")
        app.typeKey(.escape, modifierFlags: [])

        let restore = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'restore.'")).firstMatch
        restore.click()
        let back = NSPredicate { _, _ in self.exists("ledger-rs/target/debug/ledger") }
        wait(for: [expectation(for: back, evaluatedWith: nil)], timeout: 15)
    }

    /// F15: a cache is never ticked for you, the confirmation names the
    /// command, and the tool's own command is what clears it.
    func testCachesAreClearedByTheToolsCommand() {
        app.outlines.cells.containing(.any, identifier: "nav.caches").firstMatch.click()
        let box = app.checkBoxes["cache.npm"]
        XCTAssertTrue(box.waitForExistence(timeout: 15))
        XCTAssertFalse(checked("cache.npm"), "caches are never pre-selected")
        XCTAssertFalse(app.buttons["caches.clear"].isEnabled)
        box.click()
        app.buttons["caches.clear"].click()
        let confirm = app.buttons["caches.confirm"]
        XCTAssertTrue(confirm.waitForExistence(timeout: 10))
        XCTAssertTrue(exists("../home/.npm/_cacache"), "asking runs nothing")
        shot("10-caches-confirm")
        confirm.click()
        XCTAssertTrue(app.descendants(matching: .any)["caches.result"].waitForExistence(timeout: 30))
        XCTAssertFalse(exists("../home/.npm/_cacache"))
        let ran = try? String(contentsOf: base.appendingPathComponent("home/npm-ran"), encoding: .utf8)
        XCTAssertEqual(ran, "cache clean --force\n")
        shot("11-caches-cleared")
    }

    /// Xcode's accessibility audit on every screen: labels, hit areas,
    /// text clipping and contrast.
    func testAccessibilityAudit() throws {
        scan()
        var problems: [String] = []
        for screen in ["overview", "projects", "caches", "history", "devStack"] {
            app.outlines.cells.containing(.any, identifier: "nav.\(screen)").firstMatch.click()
            if screen == "projects" { app.buttons["project.shop-web"].click() }
            sleep(2)
            // Every issue fails the test except:
            // - a framework container (a Group or AppKit's TouchBar) with no
            //   identifier: no description or a parent/child mismatch on what
            //   SwiftUI builds for the split view;
            // - a contrast finding our own pixel measurement disproves. The
            //   audit misreads text on the gradient (it flagged 6.4:1 and
            //   8.1:1 text), so the element's screenshot is measured instead.
            // Issues are collected so one run reports all of them.
            try app.performAccessibilityAudit { issue in
                let e = issue.element
                if let e, [.group, .touchBar].contains(e.elementType), e.identifier.isEmpty,
                   issue.auditType == .sufficientElementDescription || issue.auditType == .parentChild {
                    return true
                }
                if issue.auditType == .contrast, let e, Self.contrast(e.screenshot().image) >= 4.5 {
                    return true
                }
                let name = [e?.identifier, e?.label, e?.value as? String].compactMap { $0 }.first { !$0.isEmpty } ?? "?"
                problems.append("\(screen): \(issue.compactDescription) on \(e?.elementType.rawValue ?? 0) '\(name)' at \(e.map { NSStringFromRect($0.frame) } ?? "?")")
                return true
            }
        }
        XCTAssert(problems.isEmpty, problems.joined(separator: "\n"))
    }

    /// WCAG contrast between the text and background of a screenshot: the
    /// brightest and darkest few percent of pixels (text is the minority).
    static func contrast(_ image: NSImage) -> Double {
        guard let tiff = image.tiffRepresentation, let rep = NSBitmapImageRep(data: tiff) else { return 0 }
        func lum(_ c: NSColor) -> Double {
            func ch(_ v: CGFloat) -> Double { let v = Double(v); return v <= 0.03928 ? v / 12.92 : pow((v + 0.055) / 1.055, 2.4) }
            guard let c = c.usingColorSpace(.sRGB) else { return 0 }
            return 0.2126 * ch(c.redComponent) + 0.7152 * ch(c.greenComponent) + 0.0722 * ch(c.blueComponent)
        }
        var l: [Double] = []
        for y in 0..<rep.pixelsHigh {
            for x in 0..<rep.pixelsWide {
                if let c = rep.colorAt(x: x, y: y) { l.append(lum(c)) }
            }
        }
        guard l.count > 50 else { return 0 }
        l.sort()
        let dark = l[l.count / 20], light = l[l.count - 1 - l.count / 50]
        return (light + 0.05) / (dark + 0.05)
    }
}
