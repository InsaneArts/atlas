import AtlasUI

/// The menu bar's glyph as a test's links read and set it, so no test reaches the real menu bar item.
final class MenuBarIconChoice {
  /// The glyph now.
  var icon: MenuBarIcon
  /// Every glyph picked, in order.
  private(set) var picks: [MenuBarIcon] = []

  /// A choice starting on `icon`.
  ///
  /// - Parameter icon: The glyph at first; the Orbit by default.
  init(_ icon: MenuBarIcon = .orbit) {
    self.icon = icon
  }

  /// Picks a glyph.
  ///
  /// - Parameter icon: The glyph.
  func pick(_ icon: MenuBarIcon) {
    self.icon = icon
    picks.append(icon)
  }
}
