import SwiftUI

/// What lies over the bottom of the helper's screen: the signing-in bar while the person signs in, else the question
/// Atlas asks them, 10 inside the picture's bottom and sides, coming and going in one move in place (``AskMotion``).
struct ScreenBanners: View {
  /// The Screen, whose bar and question it draws.
  let state: ViewerScreenState
  /// Receives the answers, Done and Cancel.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.motionReduced) private var reduceMotion

  /// The bar or the question, at the bottom.
  var body: some View {
    ZStack {
      if let bar = state.signingIn {
        SigningInBar(state: bar, send: send)
          .transition(AskMotion.transition(reduceMotion: reduceMotion))
      } else if let ask = state.ask {
        ScreenAskBanner(ask: ask, send: send)
          .id(ask.id)
          .transition(AskMotion.transition(reduceMotion: reduceMotion))
      }
    }
    .padding(AskBannerLayout.edge)
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottom)
    .animation(AskMotion.room(shown: state.ask != nil, reduceMotion: reduceMotion), value: state.ask?.id)
    .animation(
      AskMotion.room(shown: state.signingIn != nil, reduceMotion: reduceMotion),
      value: state.signingIn == nil
    )
  }
}
