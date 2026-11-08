import SwiftUI

/// How the Activity list moves, each rule pure so the motion table can check it. A session or a run opens and folds
/// as one disclosure on one spring. Any other change runs in three beats so no two rows ever share a place: rows
/// leaving fade out where they stand, then the rows that stay move into place on ``Motion/state``, then the rows
/// arriving fade in where room was made. Under Reduce Motion nothing moves or scales and only opacity changes, over at
/// most 0.2 seconds.
enum ActivityMotion {
  /// How a session or a run opens or folds: its disclosure grows or shrinks on ``Motion/state``, one spring with no
  /// bounce that turns back from where it is when pressed again, revealing or covering what it holds in place as the
  /// rows below move with it; under Reduce Motion the height changes at once, and fully fitting rows appear in place.
  ///
  /// - Parameter reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The animation, or nil for none.
  static func fold(reduceMotion: Bool) -> Animation? {
    Motion.layout(Motion.state, reduceMotion: reduceMotion)
  }

  /// How the list changes as the person picks a filter or asks for more sessions or pictures: the rows that stay move
  /// into place once the rows leaving have faded (``Motion/listMove``); geometry changes at once under Reduce Motion.
  ///
  /// - Parameter reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The animation.
  static func filter(reduceMotion: Bool) -> Animation? {
    Motion.layout(Motion.listMove, reduceMotion: reduceMotion)
  }

  /// How a row comes and goes: out with a quick fade where it stands on ``Motion/swapOut``, before anything moves
  /// over it; in, once the rows moving have made room, with a fade and a drop of ``Spacing/xsPlus`` on
  /// ``Motion/listIn``; only fading under Reduce Motion, and with no transition at all while the person has scrolled
  /// down, so what they read never moves.
  ///
  /// - Parameters:
  ///   - isAtTop: Whether the list sits at its very top.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The transition.
  static func row(isAtTop: Bool, reduceMotion: Bool) -> AnyTransition {
    guard isAtTop else { return .identity }
    guard !reduceMotion else {
      return AnyTransition.opacity
        .animation(Motion.crossfadeReduced)
    }
    let dropped = AnyTransition.opacity
      .combined(with: .offset(y: -Spacing.xsPlus))
    return .asymmetric(
      insertion: dropped.animation(Motion.listIn),
      removal: AnyTransition.opacity
        .animation(Motion.swapOut)
    )
  }

  /// How a session's or a run's chevron answers its opening or folding at once, on the spring what it holds grows on:
  /// it turns on ``Motion/state``; under Reduce Motion it does not turn, and its symbol swaps in a short fade.
  ///
  /// - Parameter reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The animation.
  static func header(reduceMotion: Bool) -> Animation {
    Motion.animation(Motion.state, reduceMotion: reduceMotion, reduced: Motion.crossfadeReduced)
  }

  /// How a step's disc changes colour, as the live blue moves on to the newest step: ``Motion/crossfade``, which only
  /// changes colour, so Reduce Motion keeps it.
  static var disc: Animation { Motion.crossfade }
}
