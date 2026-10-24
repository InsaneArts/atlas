import SwiftUI

/// How the Screen comes and goes as the tab changes, and how Activity, Apps and Screenshots come in the first time
/// they show; once shown, those three are kept and fade by ``KeptPaneShift`` instead. A tab's content swaps in place,
/// as a segmented control's does: the new pane fades in over the old one at once, eased out, and the old one fades out
/// a little quicker, eased in. Nothing slides, so the words in both stay where they are. Only opacity changes, so
/// Reduce Motion keeps the same crossfade.
///
/// While the picture moves between the stage and the corner (``ScreenZoom``, ``EnvironmentValues/screenZooms``), the
/// Screen's pane arrives whole as it grows out of the corner, so its picture and the mini screen's stay one.
struct PaneShift: ViewModifier {
  /// Whether the pane comes in, rather than goes.
  let isArriving: Bool
  /// Whether the pane is at its transition's far end: not yet in, or already gone.
  let isAway: Bool
  /// Whether the pane is the Screen's, whose picture moves to and from the corner.
  let isScreen: Bool
  @Environment(\.screenZooms) private var screenZooms

  /// The pane, faded while it is away.
  func body(content: Content) -> some View {
    content
      .opacity(ScreenZoom.opacity(isArriving: isArriving, isAway: isAway, zooms: isScreen && screenZooms))
  }

  /// The panes' transition: in on ``arrival``, out on ``departure``.
  ///
  /// - Parameter isScreen: Whether the pane is the Screen's.
  /// - Returns: The transition.
  static func transition(isScreen: Bool) -> AnyTransition {
    .asymmetric(
      insertion: AnyTransition.modifier(
        active: PaneShift(isArriving: true, isAway: true, isScreen: isScreen),
        identity: PaneShift(isArriving: true, isAway: false, isScreen: isScreen)
      )
      .animation(arrival),
      removal: AnyTransition.modifier(
        active: PaneShift(isArriving: false, isAway: true, isScreen: isScreen),
        identity: PaneShift(isArriving: false, isAway: false, isScreen: isScreen)
      )
      .animation(departure)
    )
  }

  /// A new pane's fade in over the old one: at once, eased out over ``Motion/Duration/swapIn``.
  static var arrival: Animation { Motion.swapIn }

  /// An old pane's fade out as the new one comes: eased in over ``Motion/Duration/swapOut``.
  static var departure: Animation { Motion.swapOut }
}
