import SwiftUI

/// The Screen's opacity transition and the fade tokens shared with retained lists. Activity, Apps and Screenshots
/// use ``KeptPaneShift`` and ``PaneHandoff``: the outgoing list disappears before the latest destination arrives,
/// so their words never overlap. List switches change opacity in place, also under Reduce Motion.
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

  /// A pane's fade in, eased out over ``Motion/Duration/swapIn``.
  static var arrival: Animation { Motion.swapIn }

  /// An outgoing pane's fade out, eased in over ``Motion/Duration/swapOut``.
  static var departure: Animation { Motion.swapOut }
}
