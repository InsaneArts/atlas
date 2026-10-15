import AppKit
import AtlasKit
import SwiftUI

/// Atlas's look, shared by every window: one dark ground, quiet cards, white
/// for the one thing to do next, and system blue for what is happening now.
public enum AtlasStyle {
    public static let ground = Color(red: 0x16 / 255, green: 0x16 / 255, blue: 0x18 / 255)
    public static var groundColor: NSColor {
        NSColor(srgbRed: 0x16 / 255, green: 0x16 / 255, blue: 0x18 / 255, alpha: 1)
    }
    static let ink = Color(red: 0xF5 / 255, green: 0xF5 / 255, blue: 0xF7 / 255)
    static let ink2 = Color(red: 0xA1 / 255, green: 0xA1 / 255, blue: 0xA6 / 255)
    static let ink3 = Color(red: 0x8E / 255, green: 0x8E / 255, blue: 0x93 / 255)
    static let card = Color.white.opacity(0.045)
    static let line = Color.white.opacity(0.08)
    static let lineStrong = Color.white.opacity(0.16)
    static let fill = Color.white.opacity(0.08)
    static let blue = Color(red: 0x0A / 255, green: 0x84 / 255, blue: 0xFF / 255)
    static let green = Color(red: 0x30 / 255, green: 0xD1 / 255, blue: 0x58 / 255)
    static let greenInk = Color(red: 0x32 / 255, green: 0xD7 / 255, blue: 0x4B / 255)
    static let orange = Color(red: 0xFF / 255, green: 0x9F / 255, blue: 0x0A / 255)
    static let checkInk = Color(red: 0x0B / 255, green: 0x24 / 255, blue: 0x10 / 255)
}

extension Mode.Tone {
    /// The dot's colour: green when ready, blue while working, orange when
    /// it needs the person, quiet grey otherwise.
    var color: Color {
        switch self {
        case .ready: AtlasStyle.green
        case .active: AtlasStyle.blue
        case .attention: AtlasStyle.orange
        case .quiet: AtlasStyle.ink3
        }
    }
}

// MARK: - Text

/// A small uppercase label above a title.
struct Eyebrow: View {
    let text: String
    var color = AtlasStyle.ink3

    var body: some View {
        Text(text.uppercased())
            .font(.system(size: 11, weight: .semibold))
            .tracking(0.9)
            .foregroundStyle(color)
    }
}

/// A two-line title: the statement, then its quieter tail.
struct Headline: View {
    let title: String
    var tail: String?
    var size: CGFloat = 27

    var body: some View {
        VStack(spacing: 0) {
            Text(title).foregroundStyle(AtlasStyle.ink)
            if let tail { Text(tail).foregroundStyle(AtlasStyle.ink2) }
        }
        .font(.system(size: size, weight: .semibold))
        .tracking(-0.4)
        .multilineTextAlignment(.center)
        .contentTransition(.opacity)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isHeader)
    }
}

/// The sentence or two under a title.
struct Lede: View {
    let text: String

    var body: some View {
        Text(text)
            .font(.system(size: 13))
            .lineSpacing(4)
            .foregroundStyle(AtlasStyle.ink2)
            .multilineTextAlignment(.center)
            .frame(maxWidth: 450)
            .fixedSize(horizontal: false, vertical: true)
    }
}

/// Small print.
struct Note: View {
    let text: String
    var color = AtlasStyle.ink3

    var body: some View {
        Text(text)
            .font(.system(size: 11.5))
            .foregroundStyle(color)
            .multilineTextAlignment(.center)
            .fixedSize(horizontal: false, vertical: true)
    }
}

// MARK: - Buttons

enum Emphasis { case primary, glass, plain }

/// Atlas's capsule buttons: white for the next step, glass beside it,
/// plain for the quiet way out.
struct AtlasButtonStyle: ButtonStyle {
    var emphasis: Emphasis = .primary
    var large = false
    /// Take the whole width offered (side-by-side buttons share a row).
    var fill = false
    @Environment(\.isEnabled) private var enabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: large ? 14 : 13, weight: emphasis == .plain ? .regular : .semibold))
            .lineLimit(1)
            .padding(.horizontal, large ? 26 : (emphasis == .plain ? 12 : (fill ? 10 : 18)))
            .frame(maxWidth: fill ? .infinity : nil)
            .frame(height: large ? 44 : 34)
            .foregroundStyle(foreground)
            .background(Capsule().fill(background))
            .overlay(Capsule().strokeBorder(emphasis == .glass ? AtlasStyle.lineStrong : .clear, lineWidth: 0.5))
            .contentShape(Capsule())
            .scaleEffect(configuration.isPressed ? 0.97 : 1)
            .opacity(enabled ? 1 : 0.35)
            .animation(.easeOut(duration: 0.1), value: configuration.isPressed)
    }

    private var foreground: Color {
        switch emphasis {
        case .primary: AtlasStyle.ground
        case .glass: AtlasStyle.ink
        case .plain: AtlasStyle.ink2
        }
    }

    private var background: Color {
        switch emphasis {
        case .primary: AtlasStyle.ink
        case .glass: AtlasStyle.fill
        case .plain: .clear
        }
    }
}

extension Button {
    func atlas(_ emphasis: Emphasis = .primary, large: Bool = false, fill: Bool = false) -> some View {
        buttonStyle(AtlasButtonStyle(emphasis: emphasis, large: large, fill: fill))
    }
}

// MARK: - Pieces

/// A quiet rounded group of rows.
struct Card<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        VStack(spacing: 0) { content }
            .background(RoundedRectangle(cornerRadius: 12).fill(AtlasStyle.card))
            .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(AtlasStyle.line, lineWidth: 0.5))
    }
}

/// A divider one device pixel thick, so it never drops out at any scale.
struct Rule: View {
    @Environment(\.displayScale) private var scale

    var body: some View {
        Rectangle().fill(AtlasStyle.line).frame(height: 1 / max(scale, 1))
    }
}

/// A small turning arc; still when Reduce Motion is on.
struct Spinner: View {
    var size: CGFloat = 13
    @State private var turning = false
    @Environment(\.accessibilityReduceMotion) private var still

    var body: some View {
        ZStack {
            Circle().stroke(Color.white.opacity(0.15), lineWidth: 2)
            Circle().trim(from: 0, to: 0.25)
                .stroke(AtlasStyle.blue, style: StrokeStyle(lineWidth: 2, lineCap: .round))
                .rotationEffect(.degrees(turning ? 360 : 0))
        }
        .frame(width: size, height: size)
        .onAppear {
            guard !still else { return }
            withAnimation(.linear(duration: 0.9).repeatForever(autoreverses: false)) { turning = true }
        }
        .accessibilityHidden(true)
    }
}

/// A green disc with a check.
struct Check: View {
    var size: CGFloat = 18

    var body: some View {
        ZStack {
            Circle().fill(AtlasStyle.green)
            Image(systemName: "checkmark")
                .font(.system(size: size * 0.5, weight: .bold))
                .foregroundStyle(AtlasStyle.checkInk)
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
}

/// A status line in a pill: a spinner while waiting, a check when done.
struct StatusPill: View {
    let text: String
    var done = false

    var body: some View {
        HStack(spacing: 8) {
            if done { Check(size: 13) } else { Spinner(size: 12) }
            Text(text).font(.system(size: 12)).foregroundStyle(done ? AtlasStyle.greenInk : AtlasStyle.ink2)
        }
        .padding(.horizontal, 14)
        .frame(minHeight: 30)
        .background(Capsule().fill(Color.white.opacity(0.06)))
        .accessibilityElement(children: .combine)
    }
}

/// A numbered row: waiting, current (blue) or done (green check).
struct StepRow: View {
    let index: Int
    let title: String
    let detail: String
    let state: RowState

    var body: some View {
        HStack(spacing: 12) {
            ZStack {
                Circle().fill(disc)
                if state == .done {
                    Image(systemName: "checkmark").font(.system(size: 11, weight: .bold))
                        .foregroundStyle(AtlasStyle.checkInk)
                } else {
                    Text("\(index)").font(.system(size: 12, weight: .bold))
                        .foregroundStyle(state == .current ? Color.white : AtlasStyle.ink2)
                }
            }
            .frame(width: 24, height: 24)
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(state == .waiting ? AtlasStyle.ink2 : AtlasStyle.ink)
                Text(detail).font(.system(size: 12)).foregroundStyle(AtlasStyle.ink2)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .background(
            RoundedRectangle(cornerRadius: 12).fill(state == .current ? AtlasStyle.blue.opacity(0.12) : AtlasStyle.card)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 12)
                .strokeBorder(
                    state == .current ? AtlasStyle.blue.opacity(0.55) : AtlasStyle.line,
                    lineWidth: state == .current ? 1 : 0.5)
        )
        .accessibilityElement(children: .combine)
        .accessibilityValue(state == .done ? "Done" : state == .current ? "Now" : "Later")
    }

    private var disc: Color {
        switch state {
        case .done: AtlasStyle.green
        case .current: AtlasStyle.blue
        case .waiting: Color.white.opacity(0.1)
        }
    }
}

/// Where the person is in the steps.
struct Dots: View {
    let index: Int
    let count: Int

    var body: some View {
        HStack(spacing: 6) {
            ForEach(0..<count, id: \.self) { dot in
                Capsule()
                    .fill(dot == index ? AtlasStyle.blue : Color.white.opacity(0.18))
                    .frame(width: dot == index ? 20 : 6, height: 6)
            }
        }
        .animation(.spring(response: 0.45, dampingFraction: 0.8), value: index)
        .accessibilityElement()
        .accessibilityLabel("Step \(index + 1) of \(count)")
    }
}

/// The app's own icon: Atlas.app's, or (drawing outside the app, as the
/// render tests do) the icon file beside the sources.
struct AppIcon: View {
    var size: CGFloat = 104

    @MainActor static let image: NSImage = {
        if Bundle.main.bundlePath.hasSuffix(".app"), let icon = NSApp?.applicationIconImage {
            return icon
        }
        let source = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("Resources/AppIcon.icns")
        return NSImage(contentsOf: source) ?? NSImage()
    }()

    var body: some View {
        Image(nsImage: Self.image)
            .resizable()
            .interpolation(.high)
            .frame(width: size, height: size)
            .accessibilityHidden(true)
    }
}
