import SwiftUI

/// How the Activity list moves, each rule pure so the motion table can check it. A session or a run opens and folds
/// as one disclosure on one spring: what it holds fades and slides in as its key items fade out, all on that spring,
/// with the rows below moving with it. Any other change runs in three beats so no two rows ever share a place: rows
/// leaving fade out where they stand, then the rows that stay move into place on ``Motion/state``, then the rows
/// arriving fade in where room was made. Under Reduce Motion nothing moves or scales and only opacity changes, over at
/// most 0.2 seconds.
enum ActivityMotion {
  /// How a session or a run opens or folds, its pictures open in place, or a day shows its other sessions: what it
  /// holds grows or shrinks on ``Motion/state``, one spring with no bounce that turns back from where it is when pressed
  /// again, revealing or covering what it holds in place as the rows below move with it; under Reduce Motion the height
  /// changes at once and what it holds only fades.
  ///
  /// - Parameter reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The animation, or nil for none.
  static func fold(reduceMotion: Bool) -> Animation? {
    Motion.layout(Motion.state, reduceMotion: reduceMotion)
  }

  /// How the list changes as the person picks a filter: the rows that stay move into place once the rows leaving have
  /// faded (``Motion/listMove``); geometry changes at once under Reduce Motion.
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

  /// How what a session or a run holds comes in and goes as the person opens or folds it, or asks for more of a day's
  /// sessions: a fade with a drop of ``Spacing/xsPlus`` on the change's own animation, the fold's spring, with no beat
  /// of its own and no delay, so it moves with the rows it pushes down, and the folded row's key items fade the other
  /// way at the same time; only a short fade under Reduce Motion, where nothing moves.
  ///
  /// - Parameter reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The transition.
  static func disclosure(reduceMotion: Bool) -> AnyTransition {
    guard !reduceMotion else {
      return AnyTransition.opacity
        .animation(Motion.crossfadeReduced)
    }
    return AnyTransition.opacity
      .combined(with: .offset(y: -Spacing.xsPlus))
  }

  /// How a session's run comes in and goes: with the session's disclosure while the person opens it, else as any row
  /// arrives (``row(isAtTop:reduceMotion:)``); it goes with the change that takes it, the fold's spring as its session
  /// folds, so it never vanishes before the rows below have moved.
  ///
  /// - Parameters:
  ///   - disclosing: Whether the person is opening its session, or asking for its day's other sessions, just now.
  ///   - isAtTop: Whether the list sits at its very top.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The transition.
  static func run(disclosing: Bool, isAtTop: Bool, reduceMotion: Bool) -> AnyTransition {
    .asymmetric(
      insertion: disclosing
        ? disclosure(reduceMotion: reduceMotion) : row(isAtTop: isAtTop, reduceMotion: reduceMotion),
      removal: disclosure(reduceMotion: reduceMotion)
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
