import CoreGraphics

/// The sides of square marks, dots and tiles, in points, named by size as ``Spacing`` is: a `Plus` step is 2 points
/// above its named step. A size only one window or component uses lives with it.
enum IconSize {
  /// 6: a page dot's height and a small status dot.
  static let xxs: CGFloat = 6
  /// 12: a small spinner, beside a pill's or a message's text, and the Orbit beside a status, by its height (the
  /// board's 13, rounded to the panel's 12 so both windows draw one Orbit).
  static let xs: CGFloat = 12
  /// 14: a spinner in a checklist row.
  static let xsPlus: CGFloat = 14
  /// 16: a radio, and a small app icon.
  static let s: CGFloat = 16
  /// 18: a check mark, a readiness mark and the waiting spinner.
  static let sPlus: CGFloat = 18
  /// 24: a numbered step's or an activity's disc.
  static let m: CGFloat = 24
  /// 26: a symbol tile's icon in a row, such as an agent's.
  static let mPlus: CGFloat = 26
  /// 30: a symbol tile.
  static let l: CGFloat = 30
  /// 34: an account's avatar in a choice row, and an app's icon.
  static let xl: CGFloat = 34
  /// 64: a screen's leading mark, such as a done check, and an alert's icon.
  static let xxl: CGFloat = 64
}
