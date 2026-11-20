// SPDX-License-Identifier: Apache-2.0
//
// What the app pre-selects and what a selection asks the engine for. The
// engine re-checks everything; this only decides what the user starts with.

import Foundation

public enum Selection {
    /// The absolute path of an item in a project.
    public static func path(_ item: Item, in project: Project) -> String {
        URL(fileURLWithPath: project.root).appendingPathComponent(item.path).path
    }

    /// Whether the user may tick the item at all: the guard allows it.
    public static func selectable(_ item: Item) -> Bool {
        item.bytes != nil && item.refused == nil
    }

    /// Ticked when a project opens: Safe folders the guard allows, in a
    /// project that is neither in use nor pinned. Caution is never ticked
    /// for the user (F14).
    public static func defaults(for project: Project) -> Set<String> {
        guard project.verdict != .active, !project.pinned else { return [] }
        return Set(project.items
            .filter { selectable($0) && $0.kind == .safe }
            .map { path($0, in: project) })
    }

    /// Flags a clean of `picked` needs: Caution items need
    /// `--include-caution`, an active project `--include-active`.
    public static func flags(for picked: Set<String>, in project: Project) -> (caution: Bool, active: Bool) {
        let caution = project.items.contains { $0.kind == .caution && picked.contains(path($0, in: project)) }
        return (caution, project.verdict == .active)
    }

    /// Bytes the picked folders hold.
    public static func bytes(of picked: Set<String>, in project: Project) -> UInt64 {
        project.items
            .filter { picked.contains(path($0, in: project)) }
            .reduce(0) { $0 + ($1.bytes ?? 0) }
    }
}

extension ScanSummary {
    /// The engine's scan totals (`Totals::add` in the CLI), recomputed from
    /// projects after a clean, restore or pin changes them.
    public init(totalling projects: [Project]) {
        var ready = (0, UInt64(0)), caution = (0, UInt64(0)), active = (0, UInt64(0))
        for p in projects {
            for item in p.items {
                guard let b = item.bytes else { continue }
                if p.verdict == .active { active.0 += 1; active.1 += b }
                else if item.kind == .caution { caution.0 += 1; caution.1 += b }
                else { ready.0 += 1; ready.1 += b }
            }
        }
        self.init(projects: projects.count,
                  ready: Bucket(folders: ready.0, bytes: ready.1),
                  caution: Bucket(folders: caution.0, bytes: caution.1),
                  active: Bucket(folders: active.0, bytes: active.1))
    }
}

/// What AgentClean has done on this Mac, from the quarantine's runs. Local only.
public struct Stats: Sendable, Equatable {
    /// Still in quarantine, restorable.
    public var held: UInt64 = 0
    /// Purged: the space came back.
    public var freed: UInt64 = 0
    /// Purged, in runs created this calendar month.
    public var freedThisMonth: UInt64 = 0
    /// Held plus freed, by rule id (language).
    public var byRule: [String: UInt64] = [:]

    public var cleaned: UInt64 { held + freed }

    public init(runs: [Run], now: Date = Date(), calendar: Calendar = .current) {
        for run in runs {
            let thisMonth = calendar.isDate(run.created, equalTo: now, toGranularity: .month)
            for e in run.entries {
                switch e.state {
                case .moved: held += e.bytes
                case .purged:
                    freed += e.bytes
                    if thisMonth { freedThisMonth += e.bytes }
                default: continue
                }
                byRule[e.ruleId, default: 0] += e.bytes
            }
        }
    }

    /// The biggest of 1, 10, 100 and 1000 GB that `bytes` has passed.
    public static func milestone(for bytes: UInt64) -> UInt64? {
        [1_000_000_000_000, 100_000_000_000, 10_000_000_000, 1_000_000_000].first { bytes >= $0 }
    }
}
