import SwiftUI
import AppKit
import CleanCore
import AgentEngine

/// Graphite chrome that stays out of the way: colour comes from the
/// languages' own logos, status, and one mint accent for the main action.
/// Graphite in dark mode, paper in light; the app follows the system.
/// Light text colours keep at least 4.5:1 on white.
enum Brand {
    static let canvas = Color(dark: 0x14171C, light: 0xF4F6F9)
    static let surface = Color(dark: 0x1C2027, light: 0xFFFFFF)
    static let raised = Color(dark: 0x252A33, light: 0xEDF0F4)
    static let hairline = Color(dark: 0x323844, light: 0xD9DEE6)
    static let text = Color(dark: 0xE6E9EE, light: 0x161A20)
    /// Secondary text: 7.9:1 on the dark canvas; 7:1 on white.
    static let muted = Color(dark: 0xA7AFBC, light: 0x4E5766)
    static let teal = Color(dark: 0x5FE3C8, light: 0x08695A)
    static let green = Color(dark: 0x4ADE80, light: 0x166534)
    static let amber = Color(dark: 0xF5B83D, light: 0x92400E)
    static let sky = Color(dark: 0x7AB8FF, light: 0x1D6FD6)
    static let slate = Color(dark: 0xCBD5E1, light: 0x475569)
    /// Text on a filled accent button.
    static let onAccent = Color(dark: 0x0B1F1B, light: 0xFFFFFF)
}

extension Color {
    /// One colour for dark mode and one for light.
    init(dark: UInt32, light: UInt32) {
        self.init(nsColor: NSColor(name: nil) { appearance in
            let hex = appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua ? dark : light
            return NSColor(srgbRed: CGFloat((hex >> 16) & 0xFF) / 255, green: CGFloat((hex >> 8) & 0xFF) / 255,
                           blue: CGFloat(hex & 0xFF) / 255, alpha: 1)
        })
    }

    /// A tool's brand colour, darkened in light mode so pale logos stay visible.
    init(logo hex: UInt32) {
        let r = Double((hex >> 16) & 0xFF), g = Double((hex >> 8) & 0xFF), b = Double(hex & 0xFF)
        let luminance = (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255
        let k = luminance > 0.55 ? 0.45 : 0.8
        let dim = UInt32(r * k) << 16 | UInt32(g * k) << 8 | UInt32(b * k)
        self.init(dark: hex, light: dim)
    }
}

extension Color {
    init(hex: UInt32, alpha: Double = 1) {
        self.init(.sRGB, red: Double((hex >> 16) & 0xFF) / 255, green: Double((hex >> 8) & 0xFF) / 255, blue: Double(hex & 0xFF) / 255, opacity: alpha)
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
            .foregroundStyle(Brand.onAccent)
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
        .entrance(0, from: .leading)
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
