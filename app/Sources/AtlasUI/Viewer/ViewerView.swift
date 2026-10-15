import AppKit
import AtlasKit
import SwiftUI

/// The viewer window, in the onboarding's language: the helper's desktop
/// on the dark ground, one white capsule for the next step, and quiet cards
/// of rows beside it on request.
struct ViewerView: View {
    let model: ViewerModel
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    /// Below this width the sidebar would squeeze the desktop: it waits.
    static let sidebarRoom: CGFloat = 1000

    var body: some View {
        GeometryReader { proxy in
            let room = proxy.size.width >= Self.sidebarRoom
            let sidebar = model.sidebarShown && room
            VStack(spacing: 0) {
                // The header fills the title bar, whatever its height.
                ViewerHeader(model: model, room: room)
                    .frame(height: max(proxy.safeAreaInsets.top, 44))
                HStack(spacing: 0) {
                    VStack(spacing: 0) {
                        Stage(model: model)
                            .padding(.horizontal, 24)
                            .padding(.top, 10)
                            .padding(.bottom, 20)
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                        Footer(model: model)
                    }
                    .overlay {
                        if let recovery = model.recovery {
                            RecoveryAlert(model: model, recovery: recovery)
                        }
                    }
                    if sidebar {
                        HStack(spacing: 0) {
                            VerticalRule()
                            Sidebar(model: model)
                        }
                        .transition(reduceMotion ? .opacity : .move(edge: .trailing).combined(with: .opacity))
                    }
                }
            }
            .ignoresSafeArea(.container, edges: .top)
            .animation(reduceMotion ? nil : .spring(response: 0.35, dampingFraction: 1), value: sidebar)
        }
        .background(AtlasStyle.ground)
        .foregroundStyle(AtlasStyle.ink)
        .environment(\.colorScheme, .dark)
        .onChange(of: model.mode.label) { _, label in announce(label) }
        .onChange(of: model.notice) { _, notice in if let notice { announce(notice) } }
        .onChange(of: model.recovery?.title) { _, title in if let title { announce(title) } }
    }

    /// VoiceOver says what changed.
    private func announce(_ text: String) {
        AccessibilityNotification.Announcement(text).post()
    }
}

// MARK: - Header

/// The mark and the helper's state in the middle of the title bar, the
/// sidebar toggle at its end.
struct ViewerHeader: View {
    let model: ViewerModel
    /// Wide enough for the sidebar.
    var room = true

    var body: some View {
        ZStack {
            HStack {
                Spacer()
                Button {
                    model.sidebarShown.toggle()
                } label: {
                    Image(systemName: "sidebar.right")
                }
                .buttonStyle(IconButtonStyle(size: 30, on: model.sidebarShown && room))
                .keyboardShortcut("i", modifiers: [.command, .option])
                .disabled(!room)
                .help(
                    room
                        ? (model.sidebarShown ? "Hide Sidebar (⌥⌘I)" : "Show Sidebar (⌥⌘I)")
                        : "Make the window wider to show the sidebar"
                )
                .accessibilityLabel(model.sidebarShown ? "Hide sidebar" : "Show sidebar")
            }
            // Clear of the window buttons, which full screen hides.
            .padding(.leading, model.fullScreen ? 22 : 80)
            .padding(.trailing, 16)
            HStack(spacing: 10) {
                OrbitMark(size: 22)
                StatusLine(model: model)
            }
        }
    }
}

/// "Agent working · Atlas · Live", with a dot in the mode's colour.
struct StatusLine: View {
    let model: ViewerModel

    var body: some View {
        let mode = model.mode
        // Each second while pictures arrive; barely at all while paused.
        TimelineView(.periodic(from: Date(timeIntervalSinceReferenceDate: 0), by: model.paused ? 3600 : 1)) { _ in
            HStack(spacing: 8) {
                Pulse(color: mode.tone.color, pulsing: mode.mark == .working, size: 7)
                Text(mode.label).font(.system(size: 13, weight: .semibold))
                Text("\(model.account) · \(model.freshness())")
                    .font(.system(size: 13))
                    .foregroundStyle(AtlasStyle.ink3)
                    .monospacedDigit()
            }
            .lineLimit(1)
        }
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.updatesFrequently)
        .accessibilityIdentifier("status")
    }
}

/// A coloured dot that breathes while something works.
struct Pulse: View {
    let color: Color
    var pulsing = false
    var size: CGFloat = 8
    @State private var dim = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        Circle().fill(color)
            .frame(width: size, height: size)
            .opacity(pulsing && dim ? 0.35 : 1)
            .onAppear(perform: animate)
            .onChange(of: pulsing) { animate() }
            .accessibilityHidden(true)
    }

    private func animate() {
        guard pulsing, !reduceMotion else {
            dim = false
            return
        }
        withAnimation(.easeInOut(duration: 0.8).repeatForever(autoreverses: true)) { dim = true }
    }
}

/// A round glass button holding a symbol, beside the capsules.
struct IconButtonStyle: ButtonStyle {
    var size: CGFloat = 34
    var on = false
    @Environment(\.isEnabled) private var enabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: size * 0.38, weight: .semibold))
            .foregroundStyle(AtlasStyle.ink)
            .frame(width: size, height: size)
            .background(Circle().fill(on ? AtlasStyle.lineStrong : AtlasStyle.fill))
            .overlay(Circle().strokeBorder(AtlasStyle.lineStrong, lineWidth: 0.5))
            .contentShape(Circle())
            .scaleEffect(configuration.isPressed ? 0.94 : 1)
            .opacity(enabled ? 1 : 0.35)
            .animation(.easeOut(duration: 0.1), value: configuration.isPressed)
    }
}

/// A divider one device pixel wide.
struct VerticalRule: View {
    @Environment(\.displayScale) private var scale

    var body: some View {
        Rectangle().fill(AtlasStyle.line).frame(width: 1 / max(scale, 1))
    }
}

// MARK: - Stage

/// The helper's desktop at its own shape, with what is happening laid over it.
struct Stage: View {
    let model: ViewerModel

    var body: some View {
        let size = model.image?.size ?? CGSize(width: 16, height: 10)
        ZStack {
            if model.image != nil { Color.black } else { AtlasStyle.card }
            if let image = model.image {
                Image(nsImage: image)
                    .resizable()
                    .interpolation(.high)
                    .aspectRatio(contentMode: .fit)
                    .accessibilityLabel("The helper's screen")
                    .opacity(model.paused ? 0.45 : 1)
                DesktopInput(model: model)
            } else {
                NoPicture(model: model)
            }
            VStack(spacing: 8) {
                chips
                if let notice = model.notice {
                    Chip { Text(notice) }.transition(.opacity)
                }
                Spacer()
                HStack(alignment: .bottom) {
                    Captions(items: model.captions)
                    Spacer()
                }
            }
            .padding(14)
            .allowsHitTesting(false)
        }
        .aspectRatio(size, contentMode: .fit)
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .overlay {
            RoundedRectangle(cornerRadius: 12)
                .strokeBorder(
                    model.holdingControl ? AtlasStyle.blue.opacity(0.9) : AtlasStyle.line,
                    lineWidth: model.holdingControl ? 1.5 : 0.5)
        }
        .shadow(color: model.holdingControl ? AtlasStyle.blue.opacity(0.3) : .clear, radius: 14)
        .shadow(color: .black.opacity(0.5), radius: 28, y: 14)
        .animation(.easeOut(duration: 0.2), value: model.notice)
        .animation(.easeOut(duration: 0.2), value: model.holdingControl)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Helper desktop")
    }

    @ViewBuilder private var chips: some View {
        if !model.composing.isEmpty {
            Chip {
                Text("Typing").foregroundStyle(AtlasStyle.ink3)
                Text(model.composing).underline()
            }
        } else if model.holdingControl {
            Chip {
                Pulse(color: AtlasStyle.blue, size: 7)
                Text("You have control")
                Text("Control-Shift-Escape gives it back").foregroundStyle(AtlasStyle.ink3)
            }
        } else if model.paused && model.image != nil {
            Chip {
                Image(systemName: "pause.fill").font(.system(size: 10)).foregroundStyle(AtlasStyle.ink2)
                Text("Picture paused")
            }
        }
    }
}

/// A status capsule over the desktop, as the onboarding's pills.
struct Chip<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        HStack(spacing: 8) { content }
            .font(.system(size: 12.5))
            .lineLimit(1)
            .padding(.horizontal, 14)
            .frame(height: 32)
            .background(Capsule().fill(.ultraThinMaterial))
            .background(Capsule().fill(AtlasStyle.ground.opacity(0.75)))
            .overlay(Capsule().strokeBorder(AtlasStyle.lineStrong, lineWidth: 0.5))
            .environment(\.colorScheme, .dark)
            .accessibilityElement(children: .combine)
    }
}

/// No picture yet, or none to show: said as an onboarding screen says it.
struct NoPicture: View {
    let model: ViewerModel

    var body: some View {
        VStack(spacing: 14) {
            if let reason = model.unavailable {
                Eyebrow(text: "Can't show the helper", color: AtlasStyle.orange)
                Headline(title: "The helper is", tail: "out of sight.", size: 24)
                Lede(text: reason).textSelection(.enabled)
            } else if model.paused {
                Eyebrow(text: "Paused")
                Headline(title: "Picture paused.", tail: "Work in the helper continues.", size: 24)
            } else {
                Spinner(size: 18)
                Headline(title: "Connecting", tail: "to the helper.", size: 24)
                Lede(text: "The first picture arrives in a moment.")
            }
        }
        .padding(28)
        .accessibilityElement(children: .combine)
    }
}

/// The few latest actions, oldest first, over the desktop's corner.
struct Captions: View {
    let items: [ActivityItem]

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(items) { item in
                let newest = item.id == items.last?.id
                Chip {
                    if !item.ok {
                        Circle().fill(AtlasStyle.orange).frame(width: 6, height: 6)
                    } else if newest {
                        Pulse(color: AtlasStyle.blue, pulsing: true, size: 6)
                    }
                    Text(item.line).font(.system(size: 12, weight: .medium))
                    Text(item.when).font(.system(size: 12)).foregroundStyle(AtlasStyle.ink3)
                }
                .opacity(newest ? 1 : 0.6)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Latest actions")
    }
}

/// Recovery is needed before going on: a screen of its own over the desktop.
struct RecoveryAlert: View {
    let model: ViewerModel
    let recovery: Recovery

    var body: some View {
        ZStack {
            Rectangle().fill(AtlasStyle.ground.opacity(0.82))
            VStack(spacing: 14) {
                Eyebrow(text: "Needs you", color: AtlasStyle.orange)
                Headline(title: recovery.title, size: 24)
                Lede(text: recovery.reason)
                Note(text: "Recovering restarts the helper's capture and releases held keys. Nothing is repeated.")
                    .frame(maxWidth: 380)
                HStack(spacing: 10) {
                    Button("Not Now") { model.recoverLater() }.atlas(.glass)
                        .keyboardShortcut(.cancelAction)
                    Button(recovery.busy ? "Recovering…" : "Recover") { Task { await model.recover() } }.atlas()
                        .keyboardShortcut(.defaultAction)
                }
                .disabled(recovery.busy)
                .padding(.top, 8)
            }
            .padding(32)
            .accessibilityElement(children: .contain)
            .accessibilityAddTraits(.isModal)
        }
    }
}
