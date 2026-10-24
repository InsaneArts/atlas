import SwiftUI

/// Where the Activity list holds as new lines arrive at its top, and how they arrive: at the top it stays at the top,
/// so each new line fades in as it arrives while the lines below move down to make room; scrolled down, a new line
/// comes with no motion at all and the line the person scrolled to stays where it is, so what they read never moves.
enum ActivityScroll {
  /// The line the list keeps at its top, for its scroll position.
  ///
  /// - Parameters:
  ///   - line: The line the person last scrolled to the top, if any.
  ///   - isAtTop: Whether the list sits at its very top.
  /// - Returns: None at the top, so the list stays there; else the line scrolled to, which holds its place.
  static func anchor(scrolledTo line: String?, isAtTop: Bool) -> String? {
    isAtTop ? nil : line
  }

  /// How the lines below a new one move down: on ``Motion/state``, a spring with no bounce, while the list sits at its
  /// top; at once when it is scrolled down, or under Reduce Motion.
  ///
  /// - Parameters:
  ///   - isAtTop: Whether the list sits at its very top.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The animation, or nil.
  static func moving(isAtTop: Bool, reduceMotion: Bool) -> Animation? {
    guard isAtTop else { return nil }
    return Motion.animation(Motion.state, reduceMotion: reduceMotion)
  }

  /// How a new line comes in: fading in over ``Motion/Duration/fade`` while the list sits at its top, nothing moving;
  /// with no transition at all when it is scrolled down.
  ///
  /// - Parameter isAtTop: Whether the list sits at its very top.
  /// - Returns: The transition.
  static func arrival(isAtTop: Bool) -> AnyTransition {
    guard isAtTop else { return .identity }
    return AnyTransition.opacity
      .animation(Motion.fade)
  }
}
