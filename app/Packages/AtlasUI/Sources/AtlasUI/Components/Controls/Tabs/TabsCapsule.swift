import SwiftUI

/// Both capsule styles share one measured, draggable thumb and one keyboard and accessibility choice.
struct TabsCapsule<Tab: Hashable & Sendable, Accessory: View>: View {
  /// The owner's choice and labels.
  let state: CapsuleTabsState<Tab>
  /// The label's visual style.
  let style: TabsStyle
  /// Words or symbols in the full-size track.
  let face: TabsFace
  /// Receives a committed choice.
  let send: @MainActor (Tab) -> Void
  /// Content that hangs beside a label without changing its size.
  let accessory: (Tab) -> Accessory

  @State private var motion = TabsMotion()
  @State private var hovered: Int?
  @State private var pointerFocus = false
  @State private var space = UUID()
  @GestureState private var tracking = false
  @FocusState private var focused: Bool
  @Environment(\.accessibilityReduceMotion) private var systemReduced
  @Environment(\.isEnabled) private var enabled
  @Environment(\.hoverPaused) private var hoverPaused
  #if DEBUG
    @Environment(\.tabsMotionPose) private var pose
    @Environment(\.tabsReducedMotion) private var labReduced
  #endif

  /// The system preference, with a local override only in the debug motion lab.
  private var reduced: Bool {
    #if DEBUG
      labReduced ?? systemReduced
    #else
      systemReduced
    #endif
  }

  /// The clock sleeps at rest; labels retain their measured sizes throughout every interaction.
  var body: some View {
    TimelineView(.animation(paused: clockPaused)) { clock in
      let time = clock.date.timeIntervalSinceReferenceDate
      let drawing = drawing(at: time)
      rail(drawing.motion, at: drawing.time)
        .onChange(of: clock.date) { _, date in
          guard !clockPaused else { return }
          #if DEBUG
            guard Motion.Pose.moment == nil else { return }
          #endif
          motion.finish(at: date.timeIntervalSinceReferenceDate)
        }
    }
    .coordinateSpace(name: space)
    .onPreferenceChange(TabsFramesKey.self) { frames in
      reconcile(frames: frames)
    }
    .onChange(of: state) { _, _ in reconcile(frames: measuredFrames) }
    .onChange(of: reduced) { _, _ in
      motion.cancel()
      if let selectedIndex {
        motion.choose(selectedIndex, at: now, reduced: reduced, velocity: .zero)
      }
    }
    .onChange(of: enabled) { _, value in
      if !value { cancel() }
    }
    .onChange(of: tracking) { _, value in
      if !value, motion.isPressed { cancel() }
    }
    .onDisappear { cancel() }
    .transaction { $0.animation = nil }
    .focusable(enabled, interactions: .edit)
    .focused($focused)
    .focusEffectDisabled()
    .onChange(of: focused) { _, value in pointerFocus = value && motion.isPressed }
    .onKeyPress(.leftArrow) { move(-1) }
    .onKeyPress(.rightArrow) { move(1) }
    .onKeyPress(.space) {
      pointerFocus = false
      return .handled
    }
    .accessibilityRepresentation {
      Picker(state.label, selection: Binding(get: { state.selected }, set: { choose($0) })) {
        ForEach(state.tabs) { tab in
          Text(accessibilityLabel(tab))
            .tag(tab.id)
        }
      }
      .pickerStyle(.segmented)
      .accessibilityValue(state.tabs.first { $0.id == state.selected }.map(accessibilityLabel) ?? "")
      .accessibilityAdjustableAction { direction in
        switch direction {
        case .increment: _ = move(1)
        case .decrement: _ = move(-1)
        @unknown default: break
        }
      }
    }
  }

  /// The shared track, one thumb behind the labels and subtle hover fills.
  private func rail(_ drawing: TabsMotion, at time: TimeInterval) -> some View {
    let preview = drawing.isDragging ? drawing.geometry.segment(at: drawing.centre) : drawing.selected ?? selectedIndex
    return HStack(spacing: TabsLayout.trackGap) {
      ForEach(Array(state.tabs.enumerated()), id: \.element.id) { index, tab in
        label(tab)
          .font(Typography.font(style == .segment ? .callout : .body, weight: .medium))
          .foregroundStyle(preview == index ? Color.textPrimary : Color.textSecondary)
          .lineLimit(1)
          .padding(.horizontal, horizontalInset)
          .frame(height: TabsLayout.trackHeight)
          .background {
            Capsule()
              .fill(hovered == index && preview != index && !hoverPaused ? Color.fillRowHover : .clear)
          }
          .onHover { value in hovered = value ? index : (hovered == index ? nil : hovered) }
          .animation(Motion.hover(hovered == index), value: hovered == index)
          .background {
            GeometryReader { proxy in
              Color.clear
                .preference(key: TabsFramesKey.self, value: [index: proxy.frame(in: .named(space))])
            }
          }
          .help(tab.help ?? (face == .symbol ? tab.title : ""))
      }
    }
    .padding(TabsLayout.trackInset)
    .background(alignment: .topLeading) {
      if drawing.selected != nil {
        if let outgoing = drawing.outgoing {
          thumb(outgoing, pressed: false)
            .opacity(drawing.outgoingFade(at: time))
        }
        thumb(drawing.frame(at: time), pressed: drawing.isPressed)
          .opacity(drawing.fade(at: time))
      }
    }
    .background {
      Capsule()
        .fill(Color.fillControl)
    }
    .overlay {
      if focused && !pointerFocus {
        Capsule()
          .strokeBorder(Color.strokeFocus, lineWidth: Stroke.regular)
          .padding(-TabsLayout.focusGap)
          .allowsHitTesting(false)
      }
    }
    .fixedSize()
    .contentShape(Capsule())
    .opacity(enabled ? 1 : Opacity.disabled)
    .gesture(gesture, including: enabled ? .all : .none)
  }

  /// The thumb's frame does not participate in label layout.
  private func thumb(_ frame: CGRect, pressed: Bool) -> some View {
    TabsThumb(isPressed: pressed)
      .frame(width: frame.width, height: frame.height)
      .offset(x: frame.minX, y: frame.minY)
      .allowsHitTesting(false)
  }

  /// Labels keep one font and weight; counts keep their quieter colour during preview and selection.
  @ViewBuilder private func label(_ tab: CapsuleTab<Tab>) -> some View {
    HStack(spacing: Spacing.xsPlus) {
      if style == .track, face == .symbol, let symbol = tab.symbol {
        Image(systemName: symbol.rawValue)
          .font(Typography.symbol(.control, weight: .medium))
      } else {
        Text(tab.title)
      }
      if style == .segment, let count = tab.count {
        Text(verbatim: "\(count)")
          .font(Typography.digits(.caption, weight: .medium))
          .foregroundStyle(Color.textTertiary)
      }
    }
    .modifier(TabsAccessory(accessory: accessory(tab.id), gap: accessoryGap))
  }

  /// One zero-distance gesture recognises both taps and drags, so mouse-down feedback is immediate.
  private var gesture: some Gesture {
    DragGesture(minimumDistance: .zero, coordinateSpace: .named(space))
      .updating($tracking) { _, value, _ in value = true }
      .onChanged { value in
        pointerFocus = true
        motion.press(at: value.startLocation.x, time: value.time.timeIntervalSinceReferenceDate)
        motion.drag(to: value.location.x, reduced: reduced)
      }
      .onEnded { value in
        guard
          let index = motion.release(
            at: value.location.x,
            velocity: value.velocity.width,
            time: value.time.timeIntervalSinceReferenceDate,
            reduced: reduced
          ), state.tabs.indices.contains(index)
        else { return }
        send(state.tabs[index].id)
      }
  }

  /// The font's horizontal breathing room, unchanged by selection.
  private var horizontalInset: CGFloat {
    style == .segment || face == .symbol ? Spacing.sPlus : Spacing.mPlus
  }

  /// Where an accessory hangs outside the label's measured frame.
  private var accessoryGap: CGFloat {
    style == .segment || face == .symbol ? Spacing.xs : Spacing.xsPlus
  }

  /// The owner's index, or no selection.
  private var selectedIndex: Int? { state.tabs.firstIndex { $0.id == state.selected } }
  /// The latest measured label frames in drawing order.
  private var measuredFrames: [Int: CGRect] {
    Dictionary(uniqueKeysWithValues: motion.geometry.frames.enumerated().map { ($0.offset, $0.element) })
  }
  /// The same clock used by the display timeline.
  private var now: TimeInterval { Date.timeIntervalSinceReferenceDate }

  /// The explicit lab samples sleep alongside resting controls; viewer render poses keep sampling their held clock.
  private var clockPaused: Bool {
    #if DEBUG
      motion.isIdle || pose != nil
    #else
      motion.isIdle
    #endif
  }

  /// Reconciles complete measurements only, preserving the last valid track while SwiftUI lays out its siblings.
  private func reconcile(frames: [Int: CGRect]) {
    guard frames.count == state.tabs.count else { return }
    let ordered = state.tabs.indices.compactMap { frames[$0] }
    guard ordered.count == state.tabs.count else { return }
    motion.update(frames: ordered, selected: selectedIndex, at: now, reduced: reduced)
  }

  /// Chooses from accessibility using the same thumb motion as a tap or keyboard arrow.
  private func choose(_ tab: Tab) {
    guard enabled, let index = state.tabs.firstIndex(where: { $0.id == tab }) else { return }
    motion.choose(index, at: now, reduced: reduced)
    send(tab)
  }

  /// Moves one choice from the latest destination, including while a previous slide is still running.
  private func move(_ offset: Int) -> KeyPress.Result {
    guard enabled, !state.tabs.isEmpty else { return .ignored }
    pointerFocus = false
    let index = min(max((motion.selected ?? selectedIndex ?? .zero) + offset, .zero), state.tabs.count - 1)
    choose(state.tabs[index].id)
    return .handled
  }

  /// Releases cancelled pointer ownership and restores the committed choice without sending it again.
  private func cancel() {
    guard motion.isPressed else { return }
    motion.cancel()
    if let selectedIndex { motion.choose(selectedIndex, at: now, reduced: reduced, velocity: .zero) }
  }

  /// The spoken title includes the count, without changing its visual emphasis.
  private func accessibilityLabel(_ tab: CapsuleTab<Tab>) -> String {
    tab.count.map { "\(tab.title), \($0)" } ?? tab.title
  }

  /// A deterministic lab frame or the real display clock; both draw the same rail and thumb.
  private func drawing(at time: TimeInterval) -> (motion: TabsMotion, time: TimeInterval) {
    #if DEBUG
      if let pose { return (pose.motion(in: motion.geometry), pose.time) }
      if let moment = Motion.Pose.moment {
        return (motion, (motion.spring?.start ?? motion.fadeStart) + moment)
      }
    #endif
    return (motion, time)
  }
}
