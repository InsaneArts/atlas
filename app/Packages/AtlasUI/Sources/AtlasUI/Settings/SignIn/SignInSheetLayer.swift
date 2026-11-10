import SwiftUI

/// Sign In to a Site… over the whole Settings window: the scrim, which takes every click, and the card come and go
/// together as an alert does (``AlertLayer``).
struct SignInSheetLayer: View {
  /// The sheet; nil shows none.
  let state: SignInSheetState?
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void
  @Environment(\.motionReduced) private var reduceMotion

  /// The scrim and the card, while the sheet shows.
  var body: some View {
    ZStack {
      if let state {
        Color.scrim
          .transition(AlertLayer<AlertChoice>.scrimTransition(reduceMotion: reduceMotion))
        SignInSheet(state: state, send: send)
          .transition(AlertLayer<AlertChoice>.cardTransition(reduceMotion: reduceMotion))
      }
    }
    .animation(Motion.animation(Motion.fade, reduceMotion: reduceMotion, reduced: Motion.reduced), value: state == nil)
  }
}
