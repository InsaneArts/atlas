import SwiftUI

/// How the Apps list moves, each rule pure so the motion table can check it. Cards keep their order as the front app
/// changes, so nothing travels: the card in front is raised in place, and a change to the cards runs in three beats so
/// no two cards ever share a place: a card leaving fades and shrinks slightly where it stands, then the rest close or
/// open the gap on ``Motion/state``, then a card arriving fades and grows in its new place. One moving between groups
/// is gone from the first before it shows in the second. Under Reduce Motion nothing moves or scales and only opacity
/// changes, over at most 0.2 seconds.
enum AppsMotion {
  /// How the cards make room as an app opens, quits or moves between groups: on ``Motion/listMove``, once a card
  /// leaving has faded, keyed to the cards' identities; at once under Reduce Motion, where the cards only fade.
  ///
  /// - Parameter reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The animation, or nil for none.
  static func layout(reduceMotion: Bool) -> Animation? {
    Motion.layout(Motion.listMove, reduceMotion: reduceMotion)
  }

  /// How a card comes and goes, in place, from or to ``AppsMotionLayout/grown`` of its size: out with a quick fade on
  /// ``Motion/swapOut`` before the others move; in once they have made room, on ``Motion/listIn``; only fading under
  /// Reduce Motion.
  ///
  /// - Parameter reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The transition.
  static func card(reduceMotion: Bool) -> AnyTransition {
    guard !reduceMotion else {
      return AnyTransition.opacity
        .animation(Motion.crossfadeReduced)
    }
    let faded = AnyTransition.opacity
      .combined(with: .scale(scale: AppsMotionLayout.grown))
    return .asymmetric(
      insertion: faded.animation(Motion.listIn),
      removal: faded.animation(Motion.swapOut)
    )
  }

  /// How a card is raised as its app comes to the front, and lowered as another does: its fill and shadow on
  /// ``Motion/state``; a short crossfade under Reduce Motion, which only changes colour.
  ///
  /// - Parameter reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The animation.
  static func front(reduceMotion: Bool) -> Animation {
    Motion.animation(Motion.state, reduceMotion: reduceMotion, reduced: Motion.crossfadeReduced)
  }

  /// How a card's picture changes between the window's picture and the map: a fade on ``Motion/fade``, which Reduce
  /// Motion keeps. Each new frame of the window's picture replaces the last with no animation, as the live picture
  /// does.
  static var picture: Animation { Motion.fade }
}
