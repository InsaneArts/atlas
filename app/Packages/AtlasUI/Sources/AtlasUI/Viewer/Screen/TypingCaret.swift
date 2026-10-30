import SwiftUI

/// The typing chip's caret: a thin accent bar after the text being typed that blinks once a second, held
/// still under Reduce Motion and in a render.
struct TypingCaret: View {
  @Environment(\.motionReduced) private var reduceMotion
  @Environment(\.animatesArrivals) private var animates

  /// The caret, blinking where motion and the render allow.
  var body: some View {
    if Motion.loops(reduceMotion: reduceMotion), animates {
      PhaseAnimator([true, false]) { isShown in
        caret
          .opacity(isShown ? 1 : 0)
      } animation: { _ in
        .linear(duration: .zero)
          .delay(Self.half)
      }
    } else {
      caret
    }
  }

  /// The bar.
  private var caret: some View {
    Rectangle()
      .fill(Color.accent)
      .frame(width: ViewerLayout.caret.width, height: ViewerLayout.caret.height)
      .accessibilityHidden(true)
  }

  /// Half a blink: shown, then hidden, each for half a second.
  private static let half = 0.5
}

#if DEBUG
  #Preview("Blinking") {
    TypingCaret()
      .sampleSurface()
  }
#endif
