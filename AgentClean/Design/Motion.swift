// SPDX-License-Identifier: Apache-2.0
//
// Motion, doodles and characters. Every moving piece stops or shortens when
// the person turns on Reduce Motion. The characters are Open Peeps by Pablo
// Stanley (CC0), see docs/THIRD-PARTY.md; the doodles are drawn here.

import SwiftUI
import AppKit

/// The one clock behind every endless motion: a single 12 Hz timer, stopped
/// while the app is in the background or Reduce Motion is on. Only views that
/// read `t` redraw on a tick. SwiftUI's `repeatForever` animations and
/// `TimelineView(.animation)` both kept the display loop at full rate
/// instead (measured: about 30% of a core while idle).
@MainActor @Observable
final class AmbientClock {
    static let shared = AmbientClock()
    private(set) var t: Double = 0
    @ObservationIgnored private var timer: Timer?

    private init() {
        let nc = NotificationCenter.default
        nc.addObserver(forName: NSApplication.didBecomeActiveNotification, object: nil, queue: .main) { _ in
            MainActor.assumeIsolated { AmbientClock.shared.update() }
        }
        nc.addObserver(forName: NSApplication.didResignActiveNotification, object: nil, queue: .main) { _ in
            MainActor.assumeIsolated { AmbientClock.shared.update() }
        }
        NSWorkspace.shared.notificationCenter.addObserver(forName: NSWorkspace.accessibilityDisplayOptionsDidChangeNotification, object: nil, queue: .main) { _ in
            MainActor.assumeIsolated { AmbientClock.shared.update() }
        }
        update()
    }

    private func update() {
        let run = (NSApp?.isActive ?? true) && !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        if run, timer == nil {
            timer = Timer.scheduledTimer(withTimeInterval: 1 / 12, repeats: true) { _ in
                MainActor.assumeIsolated { AmbientClock.shared.t = Date.timeIntervalSinceReferenceDate }
            }
        } else if !run {
            timer?.invalidate()
            timer = nil
        }
    }
}

/// Hands `content` the ambient clock's time.
struct Ambient<Content: View>: View {
    @ViewBuilder let content: (Double) -> Content
    var body: some View { content(AmbientClock.shared.t) }
}

// MARK: - Entrances

/// Fades, lifts and un-blurs a view the first time it appears; `delay` staggers lists.
struct Entrance: ViewModifier {
    let delay: Double
    let edge: Edge
    @State private var shown = false
    @Environment(\.accessibilityReduceMotion) private var reduce

    func body(content: Content) -> some View {
        let still = shown || reduce
        content
            .opacity(shown ? 1 : 0)
            .offset(x: still ? 0 : (edge == .leading ? -24 : edge == .trailing ? 24 : 0),
                    y: still ? 0 : (edge == .bottom ? 18 : edge == .top ? -18 : 0))
            .scaleEffect(still ? 1 : 0.97)
            .blur(radius: still ? 0 : 6)
            .onAppear {
                withAnimation(reduce ? .easeOut(duration: 0.2) : .spring(response: 0.55, dampingFraction: 0.82).delay(delay)) { shown = true }
            }
    }
}

extension View {
    /// Item `index` of a list enters a little after the one before it.
    func entrance(_ index: Int = 0, from edge: Edge = .bottom) -> some View {
        modifier(Entrance(delay: min(Double(index) * 0.05, 0.5), edge: edge))
    }

    func hoverTilt(_ tint: Color) -> some View { modifier(HoverTilt(tint: tint)) }
}

/// How a whole screen arrives when the sidebar changes it.
struct PageShift: ViewModifier {
    let on: Bool
    func body(content: Content) -> some View {
        content.opacity(on ? 0 : 1).offset(x: on ? 40 : 0).blur(radius: on ? 10 : 0)
    }
}

extension AnyTransition {
    static func page(reduce: Bool) -> AnyTransition {
        reduce ? .opacity : .asymmetric(insertion: .modifier(active: PageShift(on: true), identity: PageShift(on: false)),
                                        removal: .opacity.animation(.easeOut(duration: 0.12)))
    }
}

// MARK: - Hover

/// Leans toward the pointer, lifts and lights up in its own colour.
struct HoverTilt: ViewModifier {
    let tint: Color
    @State private var point: CGPoint?
    @State private var size: CGSize = .zero
    @Environment(\.accessibilityReduceMotion) private var reduce

    func body(content: Content) -> some View {
        let dx = point.map { ($0.x / max(size.width, 1)) - 0.5 } ?? 0
        let dy = point.map { ($0.y / max(size.height, 1)) - 0.5 } ?? 0
        content
            .overlay {
                if let point {
                    RadialGradient(colors: [tint.opacity(0.22), .clear],
                                   center: UnitPoint(x: point.x / max(size.width, 1), y: point.y / max(size.height, 1)),
                                   startRadius: 0, endRadius: 220)
                        .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
                        .allowsHitTesting(false)
                }
            }
            .background(GeometryReader { g in Color.clear.onAppear { size = g.size }.onChange(of: g.size) { size = g.size } })
            .rotation3DEffect(.degrees(reduce ? 0 : 7), axis: (x: -dy, y: dx, z: 0), perspective: 0.6)
            .scaleEffect(point == nil || reduce ? 1 : 1.025)
            .shadow(color: tint.opacity(point == nil ? 0 : 0.28), radius: 22, y: 10)
            .zIndex(point == nil ? 0 : 1)
            .animation(.spring(response: 0.35, dampingFraction: 0.75), value: point)
            .onContinuousHover { phase in
                switch phase {
                case .active(let p): point = p
                case .ended: point = nil
                }
            }
    }
}

// MARK: - Backdrop

/// Slow drifting light in the colours of your languages, over a faint dot grid.
struct AnimatedBackdrop: View {
    var colors: [Color]

    var body: some View {
        ZStack {
            Brand.canvas
            Ambient { t in
                Canvas { ctx, size in
                    for (i, c) in colors.prefix(4).enumerated() {
                        let p = Double(i) * 1.9
                        let x = size.width * (0.5 + 0.42 * sin(t * 0.05 + p))
                        let y = size.height * (0.5 + 0.40 * cos(t * 0.037 + p * 1.3))
                        let r = max(size.width, size.height) * 0.42
                        ctx.fill(Path(ellipseIn: CGRect(x: x - r, y: y - r, width: 2 * r, height: 2 * r)),
                                 with: .radialGradient(Gradient(colors: [c.opacity(0.12), c.opacity(0)]),
                                                       center: CGPoint(x: x, y: y), startRadius: 0, endRadius: r))
                    }
                }
            }
            Canvas { ctx, size in
                var dots = Path()
                let step: CGFloat = 28
                var y = step / 2
                while y < size.height {
                    var x = step / 2
                    while x < size.width { dots.addEllipse(in: CGRect(x: x - 0.8, y: y - 0.8, width: 1.6, height: 1.6)); x += step }
                    y += step
                }
                ctx.fill(dots, with: .color(.white.opacity(0.04)))
            }
        }
        .ignoresSafeArea()
        .accessibilityHidden(true)
    }
}

// MARK: - Doodles

/// Hand-drawn marks, each in a unit box.
struct Doodle: Shape {
    enum Kind { case sparkle, squiggle, loops, arrow, scribbleCircle, zigzag, plus }
    let kind: Kind

    func path(in r: CGRect) -> Path {
        var p = Path()
        func pt(_ x: CGFloat, _ y: CGFloat) -> CGPoint { CGPoint(x: r.minX + x * r.width, y: r.minY + y * r.height) }
        func curve(_ steps: Int, _ f: (CGFloat) -> CGPoint) {
            for i in 0...steps {
                let q = f(CGFloat(i) / CGFloat(steps))
                if i == 0 { p.move(to: q) } else { p.addLine(to: q) }
            }
        }
        switch kind {
        case .sparkle:
            p.move(to: pt(0.5, 0))
            p.addQuadCurve(to: pt(1, 0.5), control: pt(0.56, 0.44))
            p.addQuadCurve(to: pt(0.5, 1), control: pt(0.56, 0.56))
            p.addQuadCurve(to: pt(0, 0.5), control: pt(0.44, 0.56))
            p.addQuadCurve(to: pt(0.5, 0), control: pt(0.44, 0.44))
        case .squiggle:
            curve(80) { t in pt(t, 0.5 - 0.38 * sin(t * .pi * 5) * (0.6 + 0.4 * t)) }
        case .loops:
            curve(160) { t in pt(0.06 + t * 0.82 + 0.08 * cos(t * .pi * 8 + .pi), 0.5 + 0.38 * sin(t * .pi * 8)) }
        case .arrow:
            p.move(to: pt(0.02, 0.9))
            p.addCurve(to: pt(0.92, 0.18), control1: pt(0.2, 0.2), control2: pt(0.6, 0.02))
            p.move(to: pt(0.72, 0.08))
            p.addLine(to: pt(0.93, 0.17))
            p.addLine(to: pt(0.84, 0.38))
        case .scribbleCircle:
            curve(120) { t in
                let a = t * .pi * 2.25 - .pi / 2
                let k = 1 + 0.05 * sin(t * 11)
                return pt(0.5 + 0.47 * k * cos(a), 0.5 + 0.44 * k * sin(a))
            }
        case .zigzag:
            curve(6) { t in pt(t, Int((t * 6).rounded()) % 2 == 0 ? 0.8 : 0.2) }
        case .plus:
            p.move(to: pt(0.5, 0.05)); p.addLine(to: pt(0.5, 0.95))
            p.move(to: pt(0.05, 0.5)); p.addLine(to: pt(0.95, 0.5))
        }
        return p
    }
}

/// A doodle that draws itself in, then drifts and, for sparkles, twinkles.
struct DoodleView: View {
    let kind: Doodle.Kind
    var color: Color = Brand.teal
    var lineWidth: CGFloat = 2.5
    var delay: Double = 0
    @State private var drawn: CGFloat = 0
    @Environment(\.accessibilityReduceMotion) private var reduce

    var body: some View {
        Group {
            if kind == .sparkle {
                Ambient { t in
                    let w = sin(t * 2.4 + delay * 3)
                    Doodle(kind: kind).fill(color)
                        .scaleEffect(drawn * (0.9 + 0.2 * w))
                        .rotationEffect(.degrees(5 + 15 * w))
                }
            } else {
                Doodle(kind: kind).trim(from: 0, to: drawn)
                    .stroke(color, style: StrokeStyle(lineWidth: lineWidth, lineCap: .round, lineJoin: .round))
            }
        }
        .accessibilityHidden(true)
        .allowsHitTesting(false)
        .onAppear {
            if reduce { drawn = 1; return }
            withAnimation(.easeInOut(duration: 0.9).delay(delay)) { drawn = 1 }
        }
    }
}

/// A soft lumpy shape behind a character.
struct Blob: Shape {
    var seed: Double = 1
    func path(in r: CGRect) -> Path {
        var p = Path()
        let c = CGPoint(x: r.midX, y: r.midY)
        let n = 90
        for i in 0...n {
            let a = Double(i) / Double(n) * .pi * 2
            let k = 1 + 0.06 * sin(a * 3 + seed) + 0.04 * cos(a * 5 + seed * 2)
            let q = CGPoint(x: c.x + r.width / 2 * k * 0.94 * cos(a), y: c.y + r.height / 2 * k * 0.94 * sin(a))
            if i == 0 { p.move(to: q) } else { p.addLine(to: q) }
        }
        p.closeSubpath()
        return p
    }
}

// MARK: - Characters

/// One of the Open Peeps cast on a lumpy coloured blob, bobbing gently.
struct Peep: View {
    enum Mood: String { case happy, talk, calm, wow, lol }
    let asset: String
    var size: CGFloat = 96
    var backdrop: Color = Brand.teal
    var phase: Double = 0

    init(mood: Mood, size: CGFloat = 96, backdrop: Color = Brand.teal) {
        asset = "peep-host-\(mood.rawValue)"; self.size = size; self.backdrop = backdrop
    }

    init(crew: Int, size: CGFloat = 64, backdrop: Color = Brand.teal, phase: Double = 0) {
        asset = "peep-crew-\(crew)"; self.size = size; self.backdrop = backdrop; self.phase = phase
    }

    var body: some View {
        Ambient { t in
            let w = sin(t * 3.6 + phase)
            figure
                .offset(y: size * 0.04 * w)
                .rotationEffect(.degrees(3 * w))
        }
        .animation(.spring(response: 0.4, dampingFraction: 0.6), value: asset)
        .accessibilityHidden(true)
    }

    private var figure: some View {
        ZStack {
            Blob(seed: phase + 1).fill(backdrop)
            Image(asset).resizable().scaledToFit()
                .padding(size * 0.04)
                .clipShape(Circle())
                .id(asset)
                .transition(.scale(scale: 0.6).combined(with: .opacity))
        }
        .frame(width: size, height: size)
    }
}

/// What the host says, typed out a letter at a time.
struct SpeechBubble: View {
    let text: String
    @State private var shown = 0
    @Environment(\.accessibilityReduceMotion) private var reduce

    var body: some View {
        ZStack(alignment: .topLeading) {
            Text(text).opacity(0)
            Text(String(text.prefix(shown)))
        }
        .font(.title3.weight(.medium))
        .foregroundStyle(Brand.text)
        .fixedSize(horizontal: false, vertical: true)
        .padding(.horizontal, 18).padding(.vertical, 12)
        .background(alignment: .leading) {
            ZStack(alignment: .leading) {
                RoundedRectangle(cornerRadius: 18, style: .continuous).fill(Brand.raised)
                RoundedRectangle(cornerRadius: 4).fill(Brand.raised).frame(width: 16, height: 16)
                    .rotationEffect(.degrees(45)).offset(x: -6)
            }
        }
        .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).strokeBorder(Brand.hairline))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(text)
        .task(id: text) {
            if reduce { shown = text.count; return }
            shown = 0
            for i in 0...text.count {
                shown = i
                try? await Task.sleep(for: .milliseconds(14))
            }
        }
    }
}
