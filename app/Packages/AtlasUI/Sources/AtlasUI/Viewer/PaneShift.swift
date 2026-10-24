import SwiftUI

/// How a pane comes and goes as the tab changes (v3 X3): the new one fades in from 16 points on the side its tab
/// lies, after the old one has shown alone for 50 milliseconds, so the stage is never blank (R15); the old one fades
/// out 12 points toward the other side. The side comes from ``EnvironmentValues/paneDirection``. Under Reduce Motion
/// both only crossfade, over 0.15 seconds.
struct PaneShift: ViewModifier {
  /// Whether the pane comes in, rather than goes.
  let isArriving: Bool
  /// Whether the pane is at its transition's far end: not yet in, or already gone.
  let isAway: Bool
  @Environment(\.paneDirection) private var direction
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The pane, faded and moved while it is away.
  func body(content: Content) -> some View {
    content
      .opacity(isAway ? 0 : 1)
      .offset(
        x: isAway
          ? ViewerLayout.paneOffset(isArriving: isArriving, direction: direction, reduceMotion: reduceMotion) : .zero
      )
  }

  /// The panes' transition: in on ``Motion/tab`` after the overlap, out on ``Motion/crossfadeOut``; both on
  /// ``Motion/crossfadeReduced`` under Reduce Motion.
  ///
  /// - Parameter reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The transition.
  static func transition(reduceMotion: Bool) -> AnyTransition {
    .asymmetric(
      insertion: AnyTransition.modifier(
        active: PaneShift(isArriving: true, isAway: true),
        identity: PaneShift(isArriving: true, isAway: false)
      )
      .animation(arrival(reduceMotion: reduceMotion)),
      removal: AnyTransition.modifier(
        active: PaneShift(isArriving: false, isAway: true),
        identity: PaneShift(isArriving: false, isAway: false)
      )
      .animation(Motion.animation(Motion.crossfadeOut, reduceMotion: reduceMotion, reduced: Motion.crossfadeReduced))
    )
  }

  /// A new pane's animation: the tab spring after the overlap, or the short crossfade under Reduce Motion.
  ///
  /// - Parameter reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The animation.
  static func arrival(reduceMotion: Bool) -> Animation {
    Motion.animation(
      Motion.tab.delay(ViewerLayout.paneOverlap),
      reduceMotion: reduceMotion,
      reduced: Motion.crossfadeReduced
    )
  }
}
