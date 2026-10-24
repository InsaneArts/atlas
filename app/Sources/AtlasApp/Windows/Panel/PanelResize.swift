import AtlasUI
import Foundation

/// How the panel's popover takes a new height as Recent or the line come and go (v3 P10, X6): easing over
/// ``Motion/Duration/state`` while it shows, at once while it is closed or opening and under Reduce Motion.
enum PanelResize: Equatable {
  /// The popover takes its new size at once.
  case instant
  /// The popover eases to its new size over this many seconds.
  case eased(TimeInterval)

  /// Whether a shown panel eases its height at all. The fallback, if `NSPopover` stutters as SwiftUI resizes it, is
  /// false: every change lands at once and only the panel's own fades stay (v3 P10's risk).
  static let eases = true

  /// How a new size lands.
  ///
  /// - Parameters:
  ///   - isShown: Whether the panel shows now.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  ///   - eases: Whether a shown panel eases its height; ``eases`` unless a test says otherwise.
  /// - Returns: At once, or easing over ``Motion/Duration/state``.
  static func change(isShown: Bool, reduceMotion: Bool, eases: Bool = Self.eases) -> PanelResize {
    guard isShown, eases, Motion.loops(reduceMotion: reduceMotion) else { return .instant }
    return .eased(Motion.Duration.state.seconds)
  }
}
