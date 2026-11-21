import SwiftUI
import AgentEngine

struct HistoryView: View {
    @Environment(AppModel.self) private var model
    @State private var purging: Run?
    @State private var purgeBytes: UInt64 = 0
    @State private var olderThan: Int?
    @State private var olderBytes: UInt64 = 0

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                HStack(alignment: .top) {
                    ScreenHeader(title: "History", subtitle: "Every clean, newest first. Restore puts folders back; purge frees the space for good.")
                    Spacer()
                    Menu("Free older runs") {
                        ForEach([1, 7, 30], id: \.self) { days in
                            Button("Older than \(days) \(days == 1 ? "day" : "days")…") {
                                Task {
                                    olderBytes = await model.purge(olderThanDays: days, execute: false) ?? 0
                                    olderThan = days
                                }
                            }
                        }
                    }
                    .fixedSize()
                    .disabled(model.stats.held == 0)
                    // macOS 15 does not take the label from the menu's title.
                    .accessibilityLabel("Free older runs")
                    .accessibilityIdentifier("purgeOlder")
                }
                totals
                if model.runs.isEmpty {
                    Text("No cleans yet.").foregroundStyle(Brand.muted).padding(.top, 40).frame(maxWidth: .infinity)
                }
                ForEach(Array(model.runs.enumerated()), id: \.element.id) { i, run in
                    runCard(run).entrance(i + 1)
                }
            }
            .padding(28)
        }
        .task { await model.loadHistory() }
        .confirmationDialog(
            olderBytes == 0 ? "Nothing that old is held" : "Permanently delete \(bytes(olderBytes)) held for \(olderThan ?? 0)+ days?",
            isPresented: Binding(get: { olderThan != nil }, set: { if !$0 { olderThan = nil } })
        ) {
            if olderBytes > 0 {
                Button("Purge \(bytes(olderBytes))", role: .destructive) {
                    if let days = olderThan { Task { _ = await model.purge(olderThanDays: days, execute: true) } }
                }
            }
        } message: {
            Text("This cannot be undone. The projects can still rebuild these folders.")
        }
        .confirmationDialog(
            "Permanently delete \(bytes(purgeBytes)) from quarantine?",
            isPresented: Binding(get: { purging != nil }, set: { if !$0 { purging = nil } })
        ) {
            Button("Purge \(bytes(purgeBytes))", role: .destructive) {
                if let run = purging { Task { await model.purge(run) } }
            }
            .accessibilityIdentifier("confirmPurge")
        } message: {
            Text("This cannot be undone. The projects can still rebuild these folders.")
        }
    }

    private var totals: some View {
        let s = model.stats
        return HStack(spacing: 12) {
            total("Held, can undo", s.held, Brand.amber)
            total("Freed for good", s.freed, Brand.green)
            total("Freed this month", s.freedThisMonth, Brand.teal)
        }
    }

    private func total(_ label: String, _ value: UInt64, _ tint: Color) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(bytes(value)).font(.figure(26)).foregroundStyle(Brand.text)
                .contentTransition(.numericText(value: Double(value)))
            Text(label).font(.callout).foregroundStyle(tint)
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .glassCard()
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isStaticText)
    }

    private func runCard(_ run: Run) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text(run.created.formatted(date: .abbreviated, time: .shortened)).font(.headline).foregroundStyle(Brand.text)
                Text(run.created.relative).foregroundStyle(Brand.muted)
                Spacer()
                Text("\(bytes(run.bytesHeld)) held").monospacedDigit().foregroundStyle(Brand.muted)
                Button("Restore all") { Task { await model.restore(run) } }
                    .buttonStyle(SecondaryButtonStyle())
                    .disabled(!run.entries.contains { $0.state == .moved })
                    .accessibilityIdentifier("restore.\(run.runId)")
                Button("Purge…") {
                    Task {
                        purgeBytes = await model.purgePreview(run) ?? run.bytesHeld
                        purging = run
                    }
                }
                .buttonStyle(SecondaryButtonStyle())
                .disabled(run.bytesHeld == 0)
                .accessibilityIdentifier("purge.\(run.runId)")
            }
            ForEach(Array(run.entries.enumerated()), id: \.offset) { i, e in
                HStack(spacing: 10) {
                    Text(e.state.rawValue.capitalized)
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(e.state == .moved ? Brand.teal : Brand.slate)
                        .frame(width: 70, alignment: .leading)
                    Text(tildePath(e.original))
                        .font(.callout.monospaced()).foregroundStyle(Brand.text)
                        .lineLimit(1).truncationMode(.middle)
                    if let note = e.note { Text(note).font(.caption).foregroundStyle(Brand.amber) }
                    Spacer()
                    Text(bytes(e.bytes)).monospacedDigit().foregroundStyle(Brand.muted)
                    if e.state == .moved {
                        Button("Restore") { Task { await model.restore(run, item: i) } }
                            .buttonStyle(.borderless)
                    }
                }
                .font(.callout)
            }
        }
        .padding(16)
        .glassCard(radius: 14)
    }
}
