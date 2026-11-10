import SwiftUI

/// How a question comes and goes, in the panel and over the viewer's Screen: in one move, in place, with no delay. It
/// arrives fading in as it grows from a little smaller on ``Motion/present`` while the room opens on ``Motion/state``,
/// and leaves the exact reverse on ``Motion/close``, the content after it moving up with it. Under Reduce Motion it
/// only fades, and the room opens and closes at once.
enum AskMotion {
  /// 0.97: how small a question starts as it comes, and ends as it goes.
  static let scale = AlertCardScale.leaving

  /// How a question comes and goes.
  ///
  /// - Parameter reduceMotion: Whether Reduce Motion is on.
  /// - Returns: A scale and a fade on their own springs, or a fade alone.
  static func transition(reduceMotion: Bool) -> AnyTransition {
    guard !reduceMotion else {
      return AnyTransition.opacity
        .animation(Motion.crossfadeReduced)
    }
    let moving = AnyTransition.scale(scale: scale)
      .combined(with: .opacity)
    return .asymmetric(
      insertion: moving.animation(Motion.present),
      removal: moving.animation(Motion.close)
    )
  }

  /// How the content around a question moves as it comes or goes: on ``Motion/state`` as one comes, on
  /// ``Motion/close`` as the last goes, at once under Reduce Motion.
  ///
  /// - Parameters:
  ///   - shown: Whether a question shows after the change.
  ///   - reduceMotion: Whether Reduce Motion is on.
  /// - Returns: The animation, or nil for none.
  static func room(shown: Bool, reduceMotion: Bool) -> Animation? {
    Motion.animation(shown ? Motion.state : Motion.close, reduceMotion: reduceMotion)
  }
}
