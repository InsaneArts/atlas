import SwiftUI

/// Pops a mark in as it appears (v3 G9, `Motion.pop`): from a little under half its size to whole, with a fade. Under
/// Reduce Motion it fades only; where arrivals do not play, such as a render, it shows whole at once.
struct PopIn: ViewModifier {
  @State private var hasArrived = false
  @Environment(\.animatesArrivals) private var animatesArrivals
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The content, popping in as it appears.
  ///
  /// - Parameter content: The mark.
  /// - Returns: The mark, popping in.
  func body(content: Content) -> some View {
    content
      .scaleEffect(isSettled || reduceMotion ? 1 : Layout.arrivingScale)
      .opacity(isSettled ? 1 : 0)
      .onAppear {
        withAnimation(Motion.animation(Motion.pop, reduceMotion: reduceMotion, reduced: Motion.reduced)) {
          hasArrived = true
        }
      }
  }

  /// Whether the mark shows whole: once it has arrived, or at once where nothing plays.
  private var isSettled: Bool {
    hasArrived || !animatesArrivals
  }
}

/// The mark's own measures (v3's `scale(.4)` → `scale(1)`).
private enum Layout {
  /// 0.4: how small the mark starts.
  static let arrivingScale: CGFloat = 0.4
}
