import SwiftUI

/// How the Activity list moves, each rule pure so the motion table can check it. A moment opens and folds as one
/// disclosure on one spring. Any other change runs in three beats so no two rows ever share a place: rows leaving fade
/// out where they stand, then the rows that stay move into place on ``Motion/state``, then the rows arriving fade in
/// where room was made. Under Reduce Motion nothing moves or scales
/// and only opacity changes, over at most 0.2 seconds.
enum ActivityMotion {
  /// How a moment opens or folds, and how "Show N more" opens the rest: its disclosure grows or shrinks on
  /// ``Motion/state``, one spring with no bounce that turns back from where it is when pressed again, revealing or
  /// covering its steps in place as the moments below move with it; under Reduce Motion the height changes at once,
  /// and fully fitting steps appear in place.
  ///
  /// - Parameter reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The animation, or nil for none.
  static func fold(reduceMotion: Bool) -> Animation? {
    Motion.layout(Motion.state, reduceMotion: reduceMotion)
  }

  /// How the list changes as the person picks a filter or asks for more steps: the rows that stay move into place
  /// once the rows leaving have faded (``Motion/listMove``); geometry changes at once under Reduce Motion.
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

  /// How a row under an open moment's header comes and goes while the moment stays open: the newest, at the top, as
  /// any row comes into the list (``row(isAtTop:reduceMotion:)``). Other rows wait for the departing link to fade on
  /// ``Motion/swapOut`` before fading in, so Show more never overprints its replacement. Only opacity changes.
  ///
  /// - Parameters:
  ///   - index: The row's place under the header, the newest first.
  ///   - isAtTop: Whether the list sits at its very top.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The transition.
  static func step(at index: Int, isAtTop: Bool, reduceMotion: Bool) -> AnyTransition {
    guard index > 0 else { return row(isAtTop: isAtTop, reduceMotion: reduceMotion) }
    guard isAtTop else { return .identity }
    return .asymmetric(
      insertion: AnyTransition.opacity
        .animation(Motion.swapInAfterOut),
      removal: AnyTransition.opacity
        .animation(Motion.swapOut)
    )
  }

  /// How a moment's header answers its opening or folding at once, on the spring its steps grow on: its chevron turns
  /// and its capsule's fill fades on ``Motion/state``; under Reduce Motion the chevron does not turn, its symbol swaps
  /// in a short fade, and the fill fades as briefly.
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
