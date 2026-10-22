import CoreGraphics

/// The sides of square marks, dots and tiles, in points.
enum IconSize {
  /// 6: a page dot's height and a small status dot.
  static let dot: CGFloat = 6
  /// 12: a small spinner, beside a pill's or a message's text, and the Orbit beside a status, by its height (the
  /// board's 13, rounded to the panel's 12 so both windows draw one Orbit).
  static let glyph: CGFloat = 12
  /// 14: a spinner in a checklist row.
  static let glyphLarge: CGFloat = 14
  /// 16: a radio, and a small app icon.
  static let radio: CGFloat = 16
  /// 18: a check mark, a readiness mark and the waiting spinner.
  static let mark: CGFloat = 18
  /// 24: a numbered step's or an activity's disc.
  static let disc: CGFloat = 24
  /// 26: an agent's icon in a row.
  static let agent: CGFloat = 26
  /// 30: a symbol tile.
  static let tile: CGFloat = 30
  /// 34: an account's avatar in a choice row, and an app's icon.
  static let avatar: CGFloat = 34
  /// 56: a large avatar, such as the account asking for access.
  static let avatarLarge: CGFloat = 56
  /// 64: a screen's leading mark, such as a done check, and an alert's icon.
  static let hero: CGFloat = 64
}
