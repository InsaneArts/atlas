import AppKit

/// Atlas's appearance, and the person's display settings that AppKit motion reads.
///
/// Every Atlas window, popover and menu follows the Mac's appearance, Light, Dark or Auto, and none sets its own, so
/// its native title bar, controls and caret match the ground its colour tokens draw, and all of it changes while it is
/// open. Only the design renders name an appearance, to draw each in turn: ``light`` and ``dark`` are the one place
/// they are named. Reduce Motion and Reduce Transparency are read as each transition starts, so a change in System
/// Settings applies to the next one. SwiftUI views read the environment's `accessibilityReduceMotion` and
/// `accessibilityReduceTransparency` instead.
enum Appearance {
  /// The dark appearance a design render draws in when asked for dark.
  static let dark = NSAppearance(named: .darkAqua)
  /// The light appearance a design render draws in otherwise.
  static let light = NSAppearance(named: .aqua)

  /// Whether Reduce Motion is on: a window then moves at once instead of gliding.
  static var reduceMotion: Bool {
    NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
  }

  /// Whether Reduce Transparency is on: an AppKit surface then draws opaque instead of see-through.
  static var reduceTransparency: Bool {
    NSWorkspace.shared.accessibilityDisplayShouldReduceTransparency
  }
}
