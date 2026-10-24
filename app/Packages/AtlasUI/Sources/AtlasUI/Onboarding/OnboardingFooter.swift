import SwiftUI

/// The bar under every setup screen and the permission panel (v3 D3): Back, or Choose Another Account in its place, on
/// the left; the track in the middle; the screen's main action on the right, the one button Return presses. It sits
/// outside the screens, so it stays put while they change: its words swap in place and the track moves (v3 S1, S10).
/// No rule above it; a scrolling screen fades out over it instead (v3 D9).
struct OnboardingFooter: View {
  /// What the bar shows.
  let footer: OnboardingFooterState
  /// The bar's height: the card's or the side panel's.
  let height: CGFloat
  /// Receives the buttons' events.
  let send: @MainActor (OnboardingEvent) -> Void
  @Environment(\.mainActionInFooter) private var mainActionInFooter
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

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

  /// One side of the bar, half of what the track leaves, holding its button at its outer edge.
  ///
  /// - Parameters:
  ///   - action: The side's button, or nil for none.
  ///   - alignment: Which edge it holds to: leading for Back, trailing for the main action.
  /// - Returns: The side.
  private func slot(_ action: OnboardingFooterAction?, alignment: Alignment) -> some View {
    ZStack(alignment: alignment) {
      if let action {
        button(action, leading: alignment == .leading)
          // A new identity for each event: SwiftUI keeps a shortcut's first action while the button's title stays
          // the same, so a Continue that now does something else would otherwise send what it did before.
          .id(action.event)
          // Only opacity changes, so Reduce Motion keeps it.
          .transition(.opacity.animation(Motion.fade))
          .animation(Self.enabling(action, reduceMotion: reduceMotion), value: action.isEnabled)
      }
    }
    .frame(maxWidth: .infinity, alignment: alignment)
  }

  /// How a button lights up or waits: a moment after the success it follows (``Motion/followUp``), else at once
  /// (``Motion/state``); under Reduce Motion, a short fade at once.
  ///
  /// - Parameters:
  ///   - action: The button.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The animation for a change to whether it can be pressed.
  nonisolated static func enabling(_ action: OnboardingFooterAction, reduceMotion: Bool) -> Animation {
    let moving = action.followsSuccess && action.isEnabled ? Motion.followUp : Motion.state
    return Motion.animation(moving, reduceMotion: reduceMotion, reduced: Motion.reduced)
  }

  /// A footer button: plain on the left, filled on the right. Escape presses Back; Return presses the main action
  /// alone, and only when its state says so.
  ///
  /// - Parameters:
  ///   - action: The button.
  ///   - leading: Whether it is Back's side.
  /// - Returns: The button.
  @ViewBuilder private func button(_ action: OnboardingFooterAction, leading: Bool) -> some View {
    if leading {
      Button(action.title.words) { send(action.event) }
        .buttonStyle(.capsule(.plain))
        .disabled(!action.isEnabled)
        // Escape goes back; it never lets a helper go.
        .keyboardShortcut(action.title == .back ? .cancelAction : nil)
    } else {
      Button(action.title.words) { send(action.event) }
        .buttonStyle(.capsule())
        .disabled(!action.isEnabled)
        .keyboardShortcut(action.isDefault ? .defaultAction : nil)
    }
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
