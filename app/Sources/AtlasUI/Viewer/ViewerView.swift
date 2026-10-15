import AppKit
import AtlasKit
import SwiftUI

/// The viewer window: the helper's desktop filling it, its state above, the
/// controls below, and an inspector beside it on request.
public struct ViewerView: View {
    @Bindable var model: ViewerModel
    @Environment(\.accessibilityReduceMotion) private var still

    /// Below this width the inspector would squeeze the desktop: it waits.
    static let inspectorRoom: CGFloat = 1000

    public init(model: ViewerModel) {
        self.model = model
    }

    public var body: some View {
        GeometryReader { proxy in
            let inspector = model.inspectorShown && proxy.size.width >= Self.inspectorRoom
            VStack(spacing: 14) {
                // The header fills the title bar, whatever its height.
                ViewerHeader(model: model, room: proxy.size.width >= Self.inspectorRoom)
                    .frame(height: max(proxy.safeAreaInsets.top, 44))
                HStack(spacing: 16) {
                    VStack(spacing: 12) {
                        Stage(model: model)
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                        Dock(model: model)
                        Text(model.hint)
                            .font(.system(size: 11.5))
                            .foregroundStyle(AtlasStyle.ink3)
                            .lineLimit(1)
                            .accessibilityIdentifier("hint")
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .overlay {
                        if let recovery = model.recovery {
                            RecoveryAlert(model: model, recovery: recovery)
                        }
                    }
                    if inspector {
                        Inspector(model: model)
                            .transition(still ? .opacity : .move(edge: .trailing).combined(with: .opacity))
                    }
                }
                .padding(.horizontal, 22)
                .padding(.bottom, 18)
            }
            .ignoresSafeArea(.container, edges: .top)
            .animation(still ? nil : .spring(response: 0.35, dampingFraction: 1), value: inspector)
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

/// Brand, the status pill and the inspector toggle, in the title bar.
struct ViewerHeader: View {
    @Bindable var model: ViewerModel
    /// Wide enough for the inspector.
    var room = true

    var body: some View {
        ZStack {
            HStack(spacing: 10) {
                Spacer()
                Button {
                    model.inspectorShown.toggle()
                } label: {
                    Image(systemName: "sidebar.right")
                        .font(.system(size: 13, weight: .medium))
                        .frame(width: 32, height: 32)
                        .background(Circle().fill(AtlasStyle.fill))
                        .overlay(Circle().strokeBorder(AtlasStyle.lineStrong, lineWidth: 0.5))
                        .contentShape(Circle())
                }
                .buttonStyle(.plain)
                .keyboardShortcut("i", modifiers: [.command, .option])
                .disabled(!room)
                .help(
                    room
                        ? (model.inspectorShown ? "Hide Inspector (⌥⌘I)" : "Show Inspector (⌥⌘I)")
                        : "Make the window wider to show the inspector"
                )
                .accessibilityLabel(model.inspectorShown ? "Hide inspector" : "Show inspector")
            }
            // Clear of the window buttons, which full screen hides.
            .padding(.leading, model.fullScreen ? 22 : 80)
            .padding(.trailing, 18)
            // The mark beside the helper's state, in the middle of the title bar.
            HStack(spacing: 12) {
                OrbitMark(size: 26)
                StatusLine(model: model)
            }
        }
    }
}

/// "Agent working · Atlas Helper · Live", with a dot in the mode's colour.
struct StatusLine: View {
    let model: ViewerModel
    @Environment(\.accessibilityReduceMotion) private var still

    var body: some View {
        let mode = model.mode
        // Each second while pictures arrive; barely at all while paused.
        TimelineView(.periodic(from: Date(timeIntervalSinceReferenceDate: 0), by: model.paused ? 3600 : 1)) { _ in
            HStack(spacing: 8) {
                Pulse(color: mode.tone.color, pulsing: mode.mark == .working)
                Text(mode.label).font(.system(size: 12.5, weight: .semibold))
                Text("· \(model.account) · \(model.freshness())")
                    .font(.system(size: 12.5))
                    .foregroundStyle(AtlasStyle.ink3)
                    .monospacedDigit()
            }
            .lineLimit(1)
            .padding(.horizontal, 16)
            .frame(height: 32)
            .background(Capsule().fill(AtlasStyle.card))
            .overlay(Capsule().strokeBorder(Color.white.opacity(0.1), lineWidth: 0.5))
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
    @Environment(\.accessibilityReduceMotion) private var still

    var body: some View {
        Circle().fill(color)
            .frame(width: size, height: size)
            .opacity(pulsing && dim ? 0.35 : 1)
            .onAppear(perform: animate)
            .onChange(of: pulsing) { animate() }
            .accessibilityHidden(true)
    }

    private func animate() {
        guard pulsing, !still else {
            dim = false
            return
        }
        withAnimation(.easeInOut(duration: 0.8).repeatForever(autoreverses: true)) { dim = true }
    }
}

// MARK: - Stage

/// The helper's desktop, letterboxed at its own shape, with what is
/// happening laid over it.
struct Stage: View {
    @Bindable var model: ViewerModel

    var body: some View {
        let size = model.image?.size ?? CGSize(width: 16, height: 10)
        ZStack {
            Color.black
            if let image = model.image {
                Image(nsImage: image)
                    .resizable()
                    .interpolation(.high)
                    .aspectRatio(contentMode: .fit)
                    .accessibilityLabel("The helper's screen")
                    .opacity(model.paused ? 0.55 : 1)
                DesktopInput(model: model)
            } else {
                Empty(model: model)
            }
            VStack(spacing: 8) {
                chips
                if let notice = model.notice {
                    Chip { Text(notice).font(.system(size: 12.5)) }
                        .transition(.opacity)
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
                    model.holdingControl ? AtlasStyle.blue : Color.white.opacity(0.14),
                    lineWidth: model.holdingControl ? 2 : 0.5)
        }
        .shadow(color: model.holdingControl ? AtlasStyle.blue.opacity(0.25) : .clear, radius: 8)
        .shadow(color: .black.opacity(0.45), radius: 30, y: 16)
        .animation(.easeOut(duration: 0.2), value: model.notice)
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
                Text("Your mouse and keyboard go to the helper")
                Text("· Control-Shift-Esc gives them back").foregroundStyle(AtlasStyle.ink3)
            }
        } else if model.paused && model.image != nil {
            Chip { Text("Picture paused") }
        } else if model.status.overview.mode == "agent" {
            Chip {
                Pulse(color: AtlasStyle.blue, pulsing: true, size: 7)
                Text("An agent is working")
            }
        }
    }
}

/// A small dark capsule over the desktop.
struct Chip<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        HStack(spacing: 8) { content }
            .font(.system(size: 12.5))
            .lineLimit(1)
            .padding(.horizontal, 14)
            .frame(height: 32)
            .background(Capsule().fill(.ultraThinMaterial))
            .background(Capsule().fill(AtlasStyle.ground.opacity(0.7)))
            .environment(\.colorScheme, .dark)
            .accessibilityElement(children: .combine)
    }
}

/// No picture yet, or none to show.
struct Empty: View {
    let model: ViewerModel

    var body: some View {
        VStack(spacing: 10) {
            if let reason = model.unavailable {
                Image(systemName: "exclamationmark.triangle.fill")
                    .font(.system(size: 26)).foregroundStyle(AtlasStyle.orange)
                    .accessibilityHidden(true)
                Text("Can't show the helper").font(.system(size: 16, weight: .semibold))
                Text(reason).font(.system(size: 12.5)).foregroundStyle(AtlasStyle.ink2)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 420)
                    .textSelection(.enabled)
            } else if model.paused {
                Text("Picture paused").font(.system(size: 16, weight: .semibold))
                Text("Work in the helper continues.").font(.system(size: 12.5)).foregroundStyle(AtlasStyle.ink2)
            } else {
                Spinner(size: 18)
                Text("Connecting to the helper").font(.system(size: 16, weight: .semibold))
                Text("The first picture arrives in a moment.").font(.system(size: 12.5))
                    .foregroundStyle(AtlasStyle.ink2)
            }
        }
        .padding(24)
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

/// Recovery is needed before going on: say so over the desktop.
struct RecoveryAlert: View {
    let model: ViewerModel
    let recovery: Recovery

    var body: some View {
        ZStack {
            Rectangle().fill(.black.opacity(0.5))
            VStack(spacing: 12) {
                Eyebrow(text: "Needs you", color: AtlasStyle.orange)
                Text(recovery.title)
                    .font(.system(size: 19, weight: .semibold))
                    .multilineTextAlignment(.center)
                    .accessibilityAddTraits(.isHeader)
                Text(recovery.reason)
                    .font(.system(size: 12.5)).foregroundStyle(AtlasStyle.ink2)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
                Text("Recovering restarts the helper's capture and releases held keys. Nothing is repeated.")
                    .font(.system(size: 12.5)).foregroundStyle(AtlasStyle.ink2)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
                HStack(spacing: 8) {
                    Button("Not Now") { model.recoverLater() }.atlas(.plain)
                        .keyboardShortcut(.cancelAction)
                    Button(recovery.busy ? "Recovering…" : "Recover") { Task { await model.recover() } }.atlas()
                        .keyboardShortcut(.defaultAction)
                }
                .disabled(recovery.busy)
                .padding(.top, 6)
            }
            .padding(.horizontal, 24)
            .padding(.top, 26)
            .padding(.bottom, 20)
            .frame(width: 360)
            .background(RoundedRectangle(cornerRadius: 16).fill(AtlasStyle.ground))
            .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder(AtlasStyle.lineStrong, lineWidth: 0.5))
            .shadow(color: .black.opacity(0.55), radius: 30, y: 12)
            .accessibilityElement(children: .contain)
            .accessibilityAddTraits(.isModal)
        }
    }
}

// MARK: - Dock

/// Watch or control, the shortcuts, recovery and pause.
struct Dock: View {
    @Bindable var model: ViewerModel

    var body: some View {
        HStack(spacing: 4) {
            Segment(title: "Watch", symbol: "eye", on: !model.holdingControl) { model.releaseControl() }
                .keyboardShortcut("1", modifiers: .command)
            Segment(title: "Control", symbol: "cursorarrow", on: model.holdingControl) {
                Task { await model.takeControl() }
            }
            .keyboardShortcut("2", modifiers: .command)
            .disabled(!model.holdingControl && !model.canTakeControl)
            Divider().frame(height: 20).padding(.horizontal, 6)
            Shortcuts(model: model)
            Divider().frame(height: 20).padding(.horizontal, 6)
            if model.recoveryDeferred {
                Button("Recover…") { model.recoverNow() }.atlas(.plain)
            }
            Button {
                model.togglePause()
            } label: {
                Image(systemName: model.paused ? "play.fill" : "pause.fill")
                    .font(.system(size: 12))
                    .frame(width: 34, height: 34)
                    .contentShape(Circle())
            }
            .buttonStyle(.plain)
            .keyboardShortcut("p", modifiers: [.command, .shift])
            .help(model.paused ? "Resume Picture (⇧⌘P)" : "Pause Picture (⇧⌘P)")
            .accessibilityLabel(model.paused ? "Resume picture" : "Pause picture")
        }
        .padding(5)
        .background(Capsule().fill(AtlasStyle.card))
        .overlay(Capsule().strokeBorder(Color.white.opacity(0.1), lineWidth: 0.5))
    }
}

/// One half of the Watch/Control switch.
struct Segment: View {
    let title: String
    let symbol: String
    let on: Bool
    let action: () -> Void
    @Environment(\.isEnabled) private var enabled

    var body: some View {
        Button(action: action) {
            Label(title, systemImage: symbol)
                .font(.system(size: 13, weight: .semibold))
                .lineLimit(1)
                .fixedSize()
                .padding(.horizontal, 16)
                .frame(height: 34)
                .foregroundStyle(on ? AtlasStyle.ground : AtlasStyle.ink2)
                .background(Capsule().fill(on ? AtlasStyle.ink : .clear))
                .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .opacity(enabled ? 1 : 0.4)
        .accessibilityAddTraits(on ? [.isSelected] : [])
        .animation(.easeOut(duration: 0.15), value: on)
    }
}

// MARK: - Inspector

/// Activity and the session's facts, beside the desktop.
struct Inspector: View {
    @Bindable var model: ViewerModel

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Picker("Inspector", selection: $model.tab) {
                Text("Activity").tag(ViewerModel.Tab.activity)
                Text("Session").tag(ViewerModel.Tab.session)
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    switch model.tab {
                    case .activity: ActivityTab(model: model)
                    case .session: SessionTab(model: model)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .scrollIndicators(.automatic)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 18)
        .frame(width: 300)
        .frame(maxHeight: .infinity, alignment: .top)
        .background(RoundedRectangle(cornerRadius: 16).fill(Color.white.opacity(0.03)))
        .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder(Color.white.opacity(0.1), lineWidth: 0.5))
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Inspector")
    }
}

/// A small uppercase heading over a group.
struct GroupTitle: View {
    let text: String

    var body: some View {
        Eyebrow(text: text).padding(.horizontal, 4).accessibilityAddTraits(.isHeader)
    }
}

struct ActivityTab: View {
    let model: ViewerModel

    var body: some View {
        let items = model.overview.activity()
        VStack(alignment: .leading, spacing: 8) {
            GroupTitle(text: "Recent actions")
            if items.isEmpty {
                Text("No recent actions").font(.system(size: 13)).foregroundStyle(AtlasStyle.ink3)
                    .padding(.horizontal, 4)
            } else {
                Card {
                    ForEach(items) { item in
                        if item.id != items.first?.id { Rule() }
                        ActivityRow(item: item)
                    }
                }
            }
        }
        Note(text: "Pictures of the helper stay in memory and are never saved.")
            .frame(maxWidth: .infinity, alignment: .leading)
            .multilineTextAlignment(.leading)
            .padding(.horizontal, 4)
    }
}

struct ActivityRow: View {
    let item: ActivityItem

    var body: some View {
        HStack(spacing: 10) {
            Text(item.ok ? String(item.who.prefix(1)) : "!")
                .font(.system(size: 11, weight: .bold))
                .foregroundStyle(item.ok ? AtlasStyle.ink : AtlasStyle.ground)
                .frame(width: 24, height: 24)
                .background(Circle().fill(item.ok ? AtlasStyle.fill : AtlasStyle.orange))
                .accessibilityHidden(true)
            Text(item.line).font(.system(size: 13))
                .foregroundStyle(item.ok ? AtlasStyle.ink : AtlasStyle.orange)
            Spacer(minLength: 6)
            Text(item.when).font(.system(size: 11.5)).foregroundStyle(AtlasStyle.ink3).monospacedDigit()
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .accessibilityElement(children: .combine)
    }
}

struct SessionTab: View {
    @Bindable var model: ViewerModel

    var body: some View {
        let overview = model.overview
        let readiness = overview.readiness
        Text(overview.reason(holdingControl: model.holdingControl))
            .font(.system(size: 13)).foregroundStyle(AtlasStyle.ink2)
            .fixedSize(horizontal: false, vertical: true)
            .padding(.horizontal, 4)
        VStack(alignment: .leading, spacing: 8) {
            GroupTitle(text: "Helper")
            Card {
                let facts = overview.facts(holdingControl: model.holdingControl)
                ForEach(facts, id: \.0) { fact in
                    if fact.0 != facts.first?.0 { Rule() }
                    FactRow(name: fact.0, value: fact.1)
                }
            }
        }
        VStack(alignment: .leading, spacing: 8) {
            GroupTitle(text: "Readiness")
            Card {
                HStack(spacing: 10) {
                    if readiness.ok {
                        Check(size: 16)
                    } else {
                        Image(systemName: "exclamationmark.circle.fill").foregroundStyle(AtlasStyle.orange)
                            .accessibilityHidden(true)
                    }
                    Text(readiness.text).font(.system(size: 13))
                    Spacer()
                }
                .padding(.horizontal, 12)
                .frame(height: 38)
                .accessibilityElement(children: .combine)
                Rule()
                FactRow(name: "Save and Open panels", value: readiness.native)
                Rule()
                DisclosureGroup("Details", isExpanded: detailsShown) {
                    VStack(alignment: .leading, spacing: 8) {
                        ForEach(overview.statusRows) { row in
                            VStack(alignment: .leading, spacing: 2) {
                                Text(row.name).font(.system(size: 12, weight: .semibold))
                                Text(row.text).font(.system(size: 12))
                                    .foregroundStyle(color(row.state))
                                    .fixedSize(horizontal: false, vertical: true)
                                    .textSelection(.enabled)
                            }
                            .accessibilityElement(children: .combine)
                        }
                    }
                    .padding(.top, 6)
                }
                .font(.system(size: 12))
                .padding(.horizontal, 12)
                .padding(.vertical, 9)
            }
        }
        VStack(alignment: .leading, spacing: 8) {
            GroupTitle(text: "Picture")
            Card {
                HStack {
                    Text("Refresh").font(.system(size: 12)).foregroundStyle(AtlasStyle.ink2)
                    Spacer()
                    Picker("Refresh", selection: $model.rate) {
                        Text("Every second").tag(1.0)
                        Text("Every 2 seconds").tag(2.0)
                        Text("Every 5 seconds").tag(5.0)
                    }
                    .labelsHidden()
                    .fixedSize()
                }
                .padding(.horizontal, 12)
                .frame(height: 38)
            }
            Note(text: pictureNote)
                .frame(maxWidth: .infinity, alignment: .leading)
                .multilineTextAlignment(.leading)
                .padding(.horizontal, 4)
        }
    }

    /// Details open by themselves while something is wrong, until the person chooses.
    private var detailsShown: Binding<Bool> {
        Binding(get: { model.detailsShown }, set: { model.detailsChosen = $0 })
    }

    /// The picture's size in pixels, which a Retina picture's points are not.
    private var pictureNote: String {
        guard let pixels = model.image?.representations.first, pixels.pixelsWide > 0 else {
            return "Pictures stay in memory."
        }
        return "\(pixels.pixelsWide) × \(pixels.pixelsHigh) pictures, kept in memory only."
    }

    private func color(_ state: StatusRow.State) -> Color {
        switch state {
        case .ready: AtlasStyle.greenInk
        case .unknown: AtlasStyle.ink2
        case .bad: AtlasStyle.orange
        }
    }
}
