import SwiftUI
import CleanCore
import AgentEngine

/// One brand: teal on a deep gradient.
enum Brand {
    static let teal = Color(hex: 0x2DD4BF)
    static let green = Color(hex: 0x34D399)
    static let amber = Color(hex: 0xFBBF24)
    static let sky = Color(hex: 0x22D3EE)
    static let slate = Color(hex: 0x94A3B8)
    static let gradient = [Color(hex: 0x0B4458), Color(hex: 0x061B2A)]
}

extension Color {
    init(hex: UInt32, alpha: Double = 1) {
        self.init(.sRGB, red: Double((hex >> 16) & 0xFF) / 255, green: Double((hex >> 8) & 0xFF) / 255, blue: Double(hex & 0xFF) / 255, opacity: alpha)
    }
}

struct ThemeBackground: View {
    var body: some View {
        ZStack {
            LinearGradient(colors: Brand.gradient, startPoint: .topLeading, endPoint: .bottomTrailing)
            RadialGradient(colors: [Brand.teal.opacity(0.22), .clear], center: .init(x: 0.75, y: 0.25), startRadius: 0, endRadius: 640)
            RadialGradient(colors: [Color(hex: 0x14B8A6).opacity(0.12), .clear], center: .init(x: 0.1, y: 0.9), startRadius: 0, endRadius: 560)
        }
        .ignoresSafeArea()
    }
}

// MARK: - Glass, with a material fallback before macOS 26

extension View {
    @ViewBuilder
    func glass<S: Shape>(tint: Color? = nil, interactive: Bool = false, in shape: S) -> some View {
        #if compiler(>=6.2)
        if #available(macOS 26, *) {
            let base = Glass.regular.tint(tint)
            self.glassEffect(interactive ? base.interactive() : base, in: shape)
        } else {
            fallback(tint: tint, in: shape)
        }
        #else
        fallback(tint: tint, in: shape)
        #endif
    }

    /// Material for macOS 14 and 15, and for SDKs without Liquid Glass.
    @ViewBuilder
    private func fallback<S: Shape>(tint: Color?, in shape: S) -> some View {
        self.background(shape.fill(.ultraThinMaterial))
            .background(shape.fill((tint ?? .clear).opacity(0.5)))
            .overlay(shape.stroke(.white.opacity(0.08)))
    }

    func glassCard(radius: CGFloat = 18, tint: Color? = nil) -> some View {
        glass(tint: (tint ?? .black).opacity(tint == nil ? 0.16 : 0.22), in: RoundedRectangle(cornerRadius: radius))
    }
}

// MARK: - Components

struct StatusChip: View {
    let text: String
    let systemImage: String
    let tint: Color
    var body: some View {
        Label(text, systemImage: systemImage)
            .font(.caption.weight(.semibold))
            .padding(.horizontal, 9).padding(.vertical, 4)
            .foregroundStyle(tint)
            .glass(tint: tint.opacity(0.25), in: Capsule())
    }
}

/// The one primary action on a screen.
struct PrimaryButtonStyle: ButtonStyle {
    var tint: Color = Brand.teal
    @Environment(\.isEnabled) private var enabled
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.headline)
            .padding(.horizontal, 22).padding(.vertical, 10)
            .foregroundStyle(.black.opacity(0.85))
            .background(Capsule().fill(tint.opacity(configuration.isPressed ? 0.75 : 0.95)))
            .opacity(enabled ? 1 : 0.4)
            .scaleEffect(configuration.isPressed ? 0.97 : 1)
    }
}

struct SecondaryButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var enabled
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.subheadline.weight(.semibold))
            .padding(.horizontal, 14).padding(.vertical, 7)
            .foregroundStyle(.white)
            .glass(interactive: true, in: Capsule())
            .opacity(enabled ? 1 : 0.4)
            .scaleEffect(configuration.isPressed ? 0.97 : 1)
    }
}

/// The scan button: a breathing teal orb.
struct BigScanButton: View {
    let title: String
    let isBusy: Bool
    let action: () -> Void
    @State private var breathe = false

    var body: some View {
        Button(action: action) {
            ZStack {
                Circle().stroke(Brand.teal.opacity(0.22), lineWidth: 1.5).frame(width: 176, height: 176)
                    .scaleEffect(breathe ? 1.08 : 0.96).opacity(breathe ? 0.3 : 0.9)
                Circle()
                    .fill(RadialGradient(colors: [Brand.teal.opacity(0.95), Brand.teal.opacity(0.45)], center: .topLeading, startRadius: 0, endRadius: 150))
                    .frame(width: 124, height: 124)
                    .shadow(color: Brand.teal.opacity(0.6), radius: breathe ? 30 : 20, y: 8)
                Text(isBusy ? "Stop" : title).font(.title3.weight(.bold)).foregroundStyle(.black.opacity(0.85))
            }
        }
        .buttonStyle(.plain)
        .onAppear {
            withAnimation(.easeInOut(duration: 2.2).repeatForever(autoreverses: true)) { breathe = true }
        }
    }
}

struct SectionLabel: View {
    let text: String
    var body: some View {
        Text(text.uppercased()).font(.caption.weight(.bold)).tracking(1.2).foregroundStyle(.white.opacity(0.55))
    }
}

struct ScreenHeader: View {
    let title: String
    let subtitle: String
    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title).font(.system(size: 28, weight: .semibold, design: .rounded))
            Text(subtitle).foregroundStyle(.secondary)
        }
    }
}

func bytes(_ n: UInt64) -> String { ByteFormatter.string(n) }

extension Verdict {
    var label: String {
        switch self {
        case .active: "Active"
        case .idle: "Idle"
        case .dormant: "Dormant"
        }
    }

    var tint: Color {
        switch self {
        case .active: Brand.amber
        case .idle: Brand.sky
        case .dormant: Brand.green
        }
    }

    var symbol: String {
        switch self {
        case .active: "bolt.fill"
        case .idle: "moon.fill"
        case .dormant: "zzz"
        }
    }
}

extension FolderClass {
    var label: String { self == .safe ? "Safe" : "Caution" }
    var tint: Color { self == .safe ? Brand.green : Brand.amber }
    var explanation: String {
        self == .safe
            ? "Rebuilt automatically by the next build."
            : "Needs a reinstall (for example npm install) before the project runs again."
    }
}

extension Date {
    var relative: String {
        let f = RelativeDateTimeFormatter()
        f.unitsStyle = .short
        return f.localizedString(for: self, relativeTo: Date())
    }
}
