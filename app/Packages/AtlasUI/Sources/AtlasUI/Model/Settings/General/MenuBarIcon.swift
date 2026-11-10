/// The glyph Atlas's menu bar item draws, as Settings offers it. The logo is the Orbit either way; Screen is there for
/// a menu bar where another app's icon looks like it.
public enum MenuBarIcon: String, Hashable, Sendable, CaseIterable {
  /// The Orbit, Atlas's default.
  case orbit
  /// The Orbit on a display.
  case screen
}
