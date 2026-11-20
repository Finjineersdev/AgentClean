import SwiftUI
import CleanCore
import AgentEngine

/// Graphite chrome that stays out of the way: colour comes from the
/// languages' own logos, status, and one mint accent for the main action.
enum Brand {
    static let canvas = Color(hex: 0x14171C)
    static let surface = Color(hex: 0x1C2027)
    static let raised = Color(hex: 0x252A33)
    static let hairline = Color(hex: 0x323844)
    static let text = Color(hex: 0xE6E9EE)
    /// Secondary text: 7.9:1 on the canvas, 7:1 on raised.
    static let muted = Color(hex: 0xA7AFBC)
    static let teal = Color(hex: 0x5FE3C8)
    static let green = Color(hex: 0x4ADE80)
    static let amber = Color(hex: 0xF5B83D)
    static let sky = Color(hex: 0x7AB8FF)
    static let slate = Color(hex: 0xCBD5E1)
}

extension Color {
    init(hex: UInt32, alpha: Double = 1) {
        self.init(.sRGB, red: Double((hex >> 16) & 0xFF) / 255, green: Double((hex >> 8) & 0xFF) / 255, blue: Double(hex & 0xFF) / 255, opacity: alpha)
    }
}

struct ThemeBackground: View {
    var body: some View {
        Brand.canvas.ignoresSafeArea()
    }
}

// MARK: - Surfaces

extension View {
    /// A flat panel with a hairline edge. Hierarchy comes from the fill
    /// (surface, then raised), not from shadows.
    func glassCard(radius: CGFloat = 12, tint: Color? = nil) -> some View {
        background(RoundedRectangle(cornerRadius: radius, style: .continuous).fill(tint.map { $0.opacity(0.12) } ?? Brand.surface))
            .overlay(RoundedRectangle(cornerRadius: radius, style: .continuous).strokeBorder(Brand.hairline, lineWidth: 1))
    }

    /// The one floating element on a screen (the action bar): Liquid Glass on
    /// macOS 26, a material before that.
    @ViewBuilder
    func floatingBar<S: Shape>(in shape: S) -> some View {
        #if compiler(>=6.2)
        if #available(macOS 26, *) {
            self.glassEffect(.regular, in: shape)
        } else {
            self.background(shape.fill(.regularMaterial)).overlay(shape.stroke(Brand.hairline))
        }
        #else
        self.background(shape.fill(.regularMaterial)).overlay(shape.stroke(Brand.hairline))
        #endif
    }
}

// MARK: - Components

/// A status pill: a coloured dot and the word, so colour is never the only cue.
struct StatusChip: View {
    let text: String
    let systemImage: String
    let tint: Color
    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: systemImage).font(.system(size: 10, weight: .bold)).accessibilityHidden(true)
            Text(text)
        }
        .font(.caption.weight(.semibold))
        .padding(.horizontal, 9).padding(.vertical, 4)
        .foregroundStyle(tint)
        .background(Capsule().fill(tint.opacity(0.14)))
    }
}

/// The one primary action on a screen.
struct PrimaryButtonStyle: ButtonStyle {
    var tint: Color = Brand.teal
    @Environment(\.isEnabled) private var enabled
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.body.weight(.semibold))
            .padding(.horizontal, 18).padding(.vertical, 8)
            .foregroundStyle(Color(hex: 0x0B1F1B))
            .background(RoundedRectangle(cornerRadius: 9, style: .continuous).fill(tint.opacity(configuration.isPressed ? 0.8 : 1)))
            .opacity(enabled ? 1 : 0.35)
            .animation(.easeOut(duration: 0.12), value: configuration.isPressed)
    }
}

struct SecondaryButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var enabled
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.callout.weight(.medium))
            .padding(.horizontal, 12).padding(.vertical, 6)
            .foregroundStyle(Brand.text)
            .background(RoundedRectangle(cornerRadius: 8, style: .continuous).fill(configuration.isPressed ? Brand.hairline : Brand.raised))
            .overlay(RoundedRectangle(cornerRadius: 8, style: .continuous).strokeBorder(Brand.hairline))
            .opacity(enabled ? 1 : 0.4)
    }
}

/// A heading inside a screen, in sentence case.
struct SectionLabel: View {
    let text: String
    var trailing: String? = nil
    var body: some View {
        HStack(alignment: .firstTextBaseline) {
            Text(text).font(.headline).foregroundStyle(Brand.text)
            Spacer()
            if let trailing { Text(trailing).font(.callout).foregroundStyle(Brand.muted).monospacedDigit() }
        }
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isHeader)
    }
}

struct ScreenHeader: View {
    let title: String
    let subtitle: String
    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title).font(.system(size: 30, weight: .bold)).tracking(-0.6).foregroundStyle(Brand.text)
                .accessibilityAddTraits(.isHeader)
            Text(subtitle).font(.callout).foregroundStyle(Brand.muted).fixedSize(horizontal: false, vertical: true)
        }
    }
}

/// A thin share-of-total bar behind a size.
struct ShareBar: View {
    let share: Double
    var tint: Color = Brand.teal
    var body: some View {
        GeometryReader { g in
            ZStack(alignment: .leading) {
                Capsule().fill(Brand.hairline)
                if share > 0 {
                    Capsule().fill(tint).frame(width: max(3, g.size.width * min(share, 1)))
                }
            }
        }
        .frame(height: 4)
        .accessibilityHidden(true)
    }
}

/// Large figures: rounded, fixed-width digits.
extension Font {
    static func figure(_ size: CGFloat) -> Font { .system(size: size, weight: .bold, design: .rounded).monospacedDigit() }
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
        if abs(timeIntervalSinceNow) < 60 { return "just now" }
        let f = RelativeDateTimeFormatter()
        f.unitsStyle = .short
        return f.localizedString(for: self, relativeTo: Date())
    }
}
