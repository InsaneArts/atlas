import SwiftUI

/// A page giving way to the next (v3 S2, X7, R9): the old page fades and slides 24 points the way the pages go over
/// ``Motion/leave``, and the new one fades in from 32 points the other side on ``Motion/pageEntry``, starting while the
/// old one is still leaving. Going back mirrors it. Under Reduce Motion the two crossfade over ``Motion/reduced`` and
/// nothing moves.
struct PageSlide: Transition {
  /// The page at a phase of its transition.
  ///
  /// - Parameters:
  ///   - content: The page.
  ///   - phase: The phase of its transition.
  /// - Returns: The page, faded and moved for the phase.
  func body(content: Content, phase: TransitionPhase) -> some View {
    content.modifier(PageSlideEffect(phase: phase))
  }

  /// The page transition a ``PageStage`` gives its pages, each phase carrying its own animation, since a window's
  /// state changes without one.
  ///
  /// - Parameter reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The slide, or a crossfade under Reduce Motion.
  static func pages(reduceMotion: Bool) -> AnyTransition {
    guard !reduceMotion else {
      return .opacity.animation(Motion.reduced)
    }
    return .asymmetric(
      insertion: AnyTransition(PageSlide())
        .animation(Motion.pageEntry),
      removal: AnyTransition(PageSlide())
        .animation(Motion.leave)
    )
  }
}
