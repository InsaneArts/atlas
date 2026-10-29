import SwiftUI

/// Where the Activity list holds as new lines arrive at its top, and how the lines below move: at the top it stays at
/// the top, so the lines below move down to make room and each new line fades in once they have
/// (``ActivityMotion/row(isAtTop:reduceMotion:)``); scrolled down, a new line comes with no motion at all and the line
/// the person scrolled to stays where it is, so what they read never moves
/// (``CardScrollRoom/anchor(scrolledTo:isAtTop:)``).
enum ActivityScroll {
  /// How the lines below a new one move down: on ``Motion/state``, a spring with no bounce, at once, while the list
  /// sits at its top; at once when it is scrolled down, or under Reduce Motion.
  ///
  /// - Parameters:
  ///   - isAtTop: Whether the list sits at its very top.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The animation, or nil.
  static func moving(isAtTop: Bool, reduceMotion: Bool) -> Animation? {
    guard isAtTop else { return nil }
    return Motion.animation(Motion.state, reduceMotion: reduceMotion)
  }
}
