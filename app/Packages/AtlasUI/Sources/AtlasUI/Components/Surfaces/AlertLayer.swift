import SwiftUI

/// An alert over a whole window, top row and tabs included: the scrim, which takes every click, and the card come in
/// together on ``Motion/present``, the card growing from 0.97 to 1 as it fades in, in place; leaving, both go on
/// ``Motion/close``, the exact reverse, a little quicker. Both are springs with no bounce, so an alert asked for again
/// as it leaves turns back from where it is. Under Reduce Motion both only fade, on ``Motion/reduced``. Nothing
/// dismisses it but its own buttons.
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

  /// How the scrim comes and goes: fading in with the card, and out a little quicker; a plain fade under Reduce
  /// Motion.
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
        .animation(Motion.present),
      removal: AnyTransition.opacity
        .animation(Motion.close)
    )
  }

  /// How the card comes and goes: growing from a little smaller as it fades in, in place, and the exact reverse as it
  /// leaves; a plain fade under Reduce Motion.
  ///
  /// - Parameter reduceMotion: Whether Reduce Motion is on.
  /// - Returns: The card's transition.
  static func cardTransition(reduceMotion: Bool) -> AnyTransition {
    guard !reduceMotion else {
      return AnyTransition.opacity
        .animation(Motion.reduced)
    }
    return .asymmetric(
      insertion: AnyTransition.scale(scale: AlertCardScale.leaving)
        .combined(with: .opacity)
        .animation(Motion.present),
      removal: AnyTransition.scale(scale: AlertCardScale.leaving)
        .combined(with: .opacity)
        .animation(Motion.close)
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
