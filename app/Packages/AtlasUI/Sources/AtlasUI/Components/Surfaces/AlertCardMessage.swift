import SwiftUI

/// What the alert means, the done message in its place once done, or, once its command was refused, why, centred in
/// orange with no mark, as the card's own words are set. The asked message is laid out unseen beneath, so the card
/// keeps its height when a shorter done message takes its place. A refusal fades in as the slot eases to its height,
/// with no bounce.
struct AlertCardMessage: View {
  /// What it means now: the asked message, or the done one.
  let message: String
  /// The asked message, whose height the slot keeps.
  let reserving: String
  /// Why its command was refused; nil until it was.
  let refusal: String?
  @Environment(\.motionReduced) private var reduceMotion

  /// The component drawn from its current state.
  var body: some View {
    ZStack(alignment: .top) {
      if let refusal {
        Text(refusal)
          .textStyle(.caption)
          .foregroundStyle(Color.textAttention)
          .multilineTextAlignment(.center)
          .fixedSize(horizontal: false, vertical: true)
          .frame(maxWidth: .infinity)
          .transition(.opacity)
      } else {
        ZStack(alignment: .top) {
          words(reserving)
            .hidden()
            .accessibilityHidden(true)
          words(message)
            .swapsText(on: message)
        }
        .transition(.opacity)
      }
    }
    .animation(Motion.animation(Motion.arrive, reduceMotion: reduceMotion, reduced: Motion.reduced), value: refusal)
  }

  /// The message's words, centred under the title.
  ///
  /// - Parameter text: The words.
  /// - Returns: The message.
  private func words(_ text: String) -> some View {
    Text(text)
      .textStyle(.caption)
      .foregroundStyle(Color.textOnModal)
      .multilineTextAlignment(.center)
      .fixedSize(horizontal: false, vertical: true)
  }
}
