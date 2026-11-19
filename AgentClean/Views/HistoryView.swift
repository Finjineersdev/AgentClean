import SwiftUI
import AgentEngine

struct HistoryView: View {
    @Environment(AppModel.self) private var model
    @State private var purging: Run?
    @State private var purgeBytes: UInt64 = 0

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                HStack {
                    ScreenHeader(title: "History", subtitle: "Every clean, newest first. Restore puts folders back; purge frees the space for good.")
                    Spacer()
                    Text("Held: \(bytes(model.runs.reduce(0) { $0 + $1.bytesHeld }))")
                        .font(.headline).monospacedDigit()
                }
                if model.runs.isEmpty {
                    Text("No cleans yet.").foregroundStyle(.secondary).padding(.top, 40).frame(maxWidth: .infinity)
                }
                ForEach(model.runs) { run in
                    runCard(run)
                }
            }
            .padding(28)
        }
        .task { await model.loadHistory() }
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

    private func runCard(_ run: Run) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text(run.created.formatted(date: .abbreviated, time: .shortened)).font(.headline)
                Text(run.created.relative).foregroundStyle(.secondary)
                Spacer()
                Text("\(bytes(run.bytesHeld)) held").monospacedDigit().foregroundStyle(.secondary)
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
                    Text((e.original as NSString).abbreviatingWithTildeInPath)
                        .lineLimit(1).truncationMode(.middle)
                    if let note = e.note { Text(note).font(.caption).foregroundStyle(Brand.amber) }
                    Spacer()
                    Text(bytes(e.bytes)).monospacedDigit().foregroundStyle(.secondary)
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
