import AppKit

/// Atlas's appearance, and the person's display settings that AppKit motion reads.
///
/// Every Atlas window, popover and menu is dark, whatever the system's setting, so its native title bar, controls and
/// caret match the dark ground it draws; ``dark`` is the one place that appearance is named. Reduce Motion and Reduce
/// Transparency are read as each transition starts, so a change in System Settings applies to the next one. SwiftUI
/// views read the environment's `accessibilityReduceMotion` and `accessibilityReduceTransparency` instead.
enum Appearance {
  /// The dark appearance every Atlas window, popover and menu draws in.
  static let dark = NSAppearance(named: .darkAqua)
  /// The light appearance: only a panel render's menu bar strip drawn light uses it.
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
