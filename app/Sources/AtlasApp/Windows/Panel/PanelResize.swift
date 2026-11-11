import AtlasUI
import Foundation

/// How the panel's window takes a new height as Recent or the line come and go: easing over
/// ``Motion/Duration/resize`` while it shows, at once while it is closed or opening and under Reduce Motion.
enum PanelResize: Equatable {
  /// The window takes its new size at once.
  case instant
  /// The window eases to its new size over this many seconds.
  case eased(TimeInterval)

  /// How a new size lands.
  ///
  /// - Parameters:
  ///   - isShown: Whether the panel shows now.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: At once, or easing over ``Motion/Duration/resize``.
  static func change(isShown: Bool, reduceMotion: Bool) -> PanelResize {
    guard isShown, Motion.loops(reduceMotion: reduceMotion) else { return .instant }
    return .eased(Motion.Duration.resize.seconds)
  }
}
