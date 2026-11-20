import SwiftUI
import AgentEngine

struct RootView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        @Bindable var model = model
        NavigationSplitView {
            List(AppModel.Screen.allCases, selection: Binding(get: { model.screen }, set: { if let s = $0 { model.screen = s } })) { screen in
                Label(screen.title, systemImage: screen.symbol)
                    .tag(screen)
                    .accessibilityIdentifier("nav.\(screen.rawValue)")
            }
            .navigationSplitViewColumnWidth(min: 180, ideal: 200)
            .safeAreaInset(edge: .bottom) {
                if model.scanning {
                    HStack(spacing: 8) {
                        ProgressView().controlSize(.small)
                        Text("Scanning… \(model.projects.count) projects").font(.caption).foregroundStyle(.secondary)
                    }
                    .padding(10)
                }
            }
        } detail: {
            ZStack {
                ThemeBackground()
                switch model.screen {
                case .overview: OverviewView()
                case .projects: ProjectsView()
                case .caches: CachesView()
                case .history: HistoryView()
                case .devStack: DevStackView()
                }
            }
        }
        .alert("Something went wrong", isPresented: Binding(get: { model.error != nil }, set: { if !$0 { model.error = nil } })) {
            Button("OK") { model.error = nil }
        } message: {
            Text(model.error ?? "")
        }
        .task {
            await model.loadConfig()
            #if DEBUG
            await QAWalkthrough.run(model)
            #endif
        }
    }
}
