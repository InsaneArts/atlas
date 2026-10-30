import SwiftUI

/// An alert over a whole window, top row and tabs included: the scrim, which takes every click, fades in over
/// ``Motion/Duration/fade`` while the card fades in and settles from 1.04 to 1 on ``Motion/arrive``, with no bounce;
/// leaving, the card fades and shrinks to 0.97 over ``Motion/Duration/dismiss`` and the scrim fades on
/// ``Motion/leave``. Under Reduce Motion
/// both only fade, on ``Motion/reduced``. Nothing dismisses it but its own buttons.
struct AlertLayer<ID: Hashable & Sendable>: View {
  /// What the alert says and offers; nil shows none.
  let state: AlertCardState<ID>?
  /// Receives the button the person presses.
  let send: @MainActor (ID) -> Void
  @Environment(\.motionReduced) private var reduceMotion

  /// The scrim and the card, while there is an alert.
  var body: some View {
    ZStack {
      if let state {
        Color.scrim
          .transition(Self.scrimTransition(reduceMotion: reduceMotion))
        AlertCard(state: state, send: send)
          .transition(Self.cardTransition(reduceMotion: reduceMotion))
      }
    }
    .animation(Motion.animation(Motion.fade, reduceMotion: reduceMotion, reduced: Motion.reduced), value: state == nil)
  }

  /// How the scrim comes and goes: a fade in, a quicker fade out; a plain fade under Reduce Motion.
  ///
  /// - Parameter reduceMotion: Whether Reduce Motion is on.
  /// - Returns: The scrim's transition.
  static func scrimTransition(reduceMotion: Bool) -> AnyTransition {
    guard !reduceMotion else {
      return AnyTransition.opacity
        .animation(Motion.reduced)
    }
    return .asymmetric(
      insertion: AnyTransition.opacity
        .animation(Motion.fade),
      removal: AnyTransition.opacity
        .animation(Motion.leave)
    )
  }

  /// How the card comes and goes: settling from a little larger, leaving a little smaller, fading both ways; a plain
  /// fade under Reduce Motion.
  ///
  /// - Parameter reduceMotion: Whether Reduce Motion is on.
  /// - Returns: The card's transition.
  static func cardTransition(reduceMotion: Bool) -> AnyTransition {
    guard !reduceMotion else {
      return AnyTransition.opacity
        .animation(Motion.reduced)
    }
    return .asymmetric(
      insertion: AnyTransition.scale(scale: AlertCardScale.arriving)
        .combined(with: .opacity)
        .animation(Motion.arrive),
      removal: AnyTransition.scale(scale: AlertCardScale.leaving)
        .combined(with: .opacity)
        .animation(Motion.dismiss)
    )
  }
}

#if DEBUG
  #Preview("Samples") {
    AlertLayer(state: AlertCardSamples.logOut) { _ in }
      .frame(width: SampleWidth.form, height: SampleWidth.form)
  }

  #Preview("Interactive") {
    @Previewable @State var shown = true
    InteractivePreview {
      Toggle(String("Shown"), isOn: $shown)
    } content: {
      AlertLayer(state: shown ? AlertCardSamples.logOut : nil) { _ in shown = false }
        .frame(width: SampleWidth.form, height: SampleWidth.form)
    }
  }
#endif
