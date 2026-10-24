import SwiftUI

/// How a pane comes and goes as the tab changes (v3 X3): the new one fades in from 16 points on the side its tab
/// lies, after the old one has shown alone for 50 milliseconds, so the stage is never blank (R15); the old one fades
/// out 12 points toward the other side. The side comes from ``EnvironmentValues/paneDirection``. Under Reduce Motion
/// both only crossfade, over 0.15 seconds.
///
/// While the picture moves between the stage and the corner (``ScreenZoom``, ``EnvironmentValues/screenZooms``), the
/// Screen's pane does not slide: it arrives whole as it grows out of the corner, or fades in place as it shrinks into
/// it, so its picture and the mini screen's stay one.
struct PaneShift: ViewModifier {
  /// Whether the pane comes in, rather than goes.
  let isArriving: Bool
  /// Whether the pane is at its transition's far end: not yet in, or already gone.
  let isAway: Bool
  /// Whether the pane is the Screen's, whose picture moves to and from the corner.
  let isScreen: Bool
  @Environment(\.paneDirection) private var direction
  @Environment(\.screenZooms) private var screenZooms
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The pane, faded and moved while it is away.
  func body(content: Content) -> some View {
    content
      .opacity(ScreenZoom.opacity(isArriving: isArriving, isAway: isAway, zooms: zooms))
      .offset(x: isAway ? offset : .zero)
  }

  /// Whether this pane's picture moves between the stage and the corner.
  private var zooms: Bool {
    isScreen && screenZooms
  }

  /// How far the pane sits from its place while it is away: none while its picture moves.
  private var offset: CGFloat {
    guard !zooms else { return .zero }
    return ViewerLayout.paneOffset(isArriving: isArriving, direction: direction, reduceMotion: reduceMotion)
  }

  /// The panes' transition: in on ``Motion/tab`` after the overlap, out on ``Motion/crossfadeOut``; both on
  /// ``Motion/crossfadeReduced`` under Reduce Motion.
  ///
  /// - Parameters:
  ///   - isScreen: Whether the pane is the Screen's.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The transition.
  static func transition(isScreen: Bool, reduceMotion: Bool) -> AnyTransition {
    .asymmetric(
      insertion: AnyTransition.modifier(
        active: PaneShift(isArriving: true, isAway: true, isScreen: isScreen),
        identity: PaneShift(isArriving: true, isAway: false, isScreen: isScreen)
      )
      .animation(arrival(reduceMotion: reduceMotion)),
      removal: AnyTransition.modifier(
        active: PaneShift(isArriving: false, isAway: true, isScreen: isScreen),
        identity: PaneShift(isArriving: false, isAway: false, isScreen: isScreen)
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
