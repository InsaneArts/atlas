import SwiftUI

/// The bar under every setup screen and the permission panel: Back, Not Now or Choose Another Account on the left; the
/// track in the middle; the screen's main action on the right, the one button Return presses. It sits outside the
/// screens, so it stays put while they change. Each side keeps one button from screen to screen: its words swap in
/// place by opacity as the capsule eases to their width, so two buttons never cross through each other. No rule above
/// it.
struct OnboardingFooter: View {
  /// What the bar shows.
  let footer: OnboardingFooterState
  /// The bar's height: the card's or the side panel's.
  let height: CGFloat
  /// Receives the buttons' events.
  let send: @MainActor (OnboardingEvent) -> Void
  @Environment(\.mainActionInFooter) private var mainActionInFooter
  @Environment(\.motionReduced) private var reduceMotion

  /// Back, the track and the main action, the track centred between the two sides.
  var body: some View {
    HStack(spacing: Spacing.s) {
      slot(footer.back, alignment: .leading)
      if let track = footer.track {
        ProgressTrack(index: track.index, count: track.count, label: track.label)
      }
      slot(next, alignment: .trailing)
    }
    .padding(.horizontal, OnboardingLayout.barInset)
    .frame(height: height)
  }

  /// The main action, unless the screen draws it (``SwiftUICore/EnvironmentValues/mainActionInFooter`` off).
  private var next: OnboardingFooterAction? {
    guard let next = footer.next, mainActionInFooter || !next.title.isScreenAction else { return nil }
    return next
  }

  /// One side of the bar, half of what the track leaves, holding its button at its outer edge. A side that gains or
  /// loses its button fades it.
  ///
  /// - Parameters:
  ///   - action: The side's button, or nil for none.
  ///   - alignment: Which edge it holds to: leading for Back, trailing for the main action.
  /// - Returns: The side.
  private func slot(_ action: OnboardingFooterAction?, alignment: Alignment) -> some View {
    ZStack(alignment: alignment) {
      if let action {
        button(action, leading: alignment == .leading)
          // Only opacity changes, so Reduce Motion keeps it.
          .transition(.opacity.animation(Motion.fade))
      }
    }
    .frame(maxWidth: .infinity, alignment: alignment)
  }

  /// A footer button: plain on the left, filled on the right. The button seen keeps its identity, so a new title
  /// swaps in place; its key is carried apart (``shortcutCarrier(_:key:)``).
  ///
  /// - Parameters:
  ///   - action: The button.
  ///   - leading: Whether it is Back's side.
  /// - Returns: The button.
  private func button(_ action: OnboardingFooterAction, leading: Bool) -> some View {
    Button {
      send(action.event)
    } label: {
      Text(action.title.words)
        .contentTransition(.opacity)
    }
    .buttonStyle(.capsule(leading ? .plain : .primary))
    .disabled(!action.isEnabled)
    .background { shortcutCarrier(action, key: leading ? Self.leadingKey(action) : Self.trailingKey(action)) }
    // The new words fade in as the capsule eases to their width: opacity and a short resize, kept under Reduce Motion.
    .animation(Motion.textSwap, value: action.title)
    .animation(Self.enabling(reduceMotion: reduceMotion), value: action.isEnabled)
  }

  /// The key that presses a button, on a button of its own that is never seen and takes a new identity for each event:
  /// SwiftUI keeps a key's first action while its button's title stays the same, so a Continue that now does
  /// something else would otherwise send what it did before.
  ///
  /// - Parameters:
  ///   - action: The button the key presses.
  ///   - key: The key, or nil for none.
  /// - Returns: The carrier, or nothing for a button with no key.
  @ViewBuilder private func shortcutCarrier(_ action: OnboardingFooterAction, key: KeyboardShortcut?) -> some View {
    if let key {
      Button(action.title.words) { send(action.event) }
        .keyboardShortcut(key)
        .disabled(!action.isEnabled)
        .frame(width: .zero, height: .zero)
        .opacity(.zero)
        .accessibilityHidden(true)
        .id(action.event)
    }
  }

  /// The key a button on the left takes: Escape for Not Now and for Back on the permission panel, where nothing is
  /// typed; Command-[ for Back on a screen, as Setup Assistant's Back, so Escape while typing a password never goes
  /// back; none for Choose Another Account, which lets a helper go.
  ///
  /// - Parameter action: The button.
  /// - Returns: Its key, or nil for none.
  nonisolated static func leadingKey(_ action: OnboardingFooterAction) -> KeyboardShortcut? {
    switch action.title {
    case .notNow: .cancelAction
    case .back where action.event == .closePanel: .cancelAction
    case .back: KeyboardShortcut("[", modifiers: .command)
    default: nil
    }
  }

  /// The key the main action takes: Return, when its state says so.
  ///
  /// - Parameter action: The button.
  /// - Returns: Return, or nil for none.
  nonisolated static func trailingKey(_ action: OnboardingFooterAction) -> KeyboardShortcut? {
    action.isDefault ? .defaultAction : nil
  }

  /// How a button lights up or waits: at once (``Motion/state``), the moment its state says it can be pressed, so it
  /// never looks unlike what a press does; under Reduce Motion, a short fade. A button that follows a success, such as
  /// Continue once the check passes, is held back by its state, not here.
  ///
  /// - Parameter reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The animation for a change to whether it can be pressed.
  nonisolated static func enabling(reduceMotion: Bool) -> Animation {
    Motion.animation(Motion.state, reduceMotion: reduceMotion, reduced: Motion.reduced)
  }
}

#if DEBUG
  #Preview("Create") {
    OnboardingFooter(footer: PreviewStates.onboardingCreate.footer, height: OnboardingLayout.footerHeight) { _ in }
      .frame(width: OnboardingLayout.card.width)
      .windowSurface()
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.onboardingWelcome
    InteractivePreview {
      Menu(String("State")) {
        ForEach(PreviewStates.onboarding) { preview in
          Button(preview.name) { state = preview.value }
        }
      }
      .fixedSize()
    } content: {
      OnboardingFooter(footer: state.footer, height: OnboardingLayout.footerHeight) { _ in }
        .frame(width: OnboardingLayout.card.width)
    }
  }
#endif
