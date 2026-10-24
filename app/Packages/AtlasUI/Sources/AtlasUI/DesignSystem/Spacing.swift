import CoreGraphics

/// Gaps and insets, on a 2-point grid, named by size as jolt's are: `xs` 4, `s` 8, `m` 12, `l` 16, `xl` 20, `xxl` 24
/// and `xxxl` 32, with `half` 2 and `hairline` 1 below them. A `Plus` step is the value 2 points above its named
/// step, for the grid's 6, 10 and 14. A value only one window uses lives in that window's layout; a value off the
/// grid that a component cannot derive is a private constant in the component, with its reason.
enum Spacing {
  /// 1: a hairline, the one step off the grid.
  static let hairline: CGFloat = 1
  /// 2: between a title and its detail.
  static let half: CGFloat = 2
  /// 4: between a headline and its reason, and a lede's line spacing.
  static let xs: CGFloat = 4
  /// 6: between page dots, and between a dot and its label.
  static let xsPlus: CGFloat = 6
  /// 8: between buttons in a row and inside a pill.
  static let s: CGFloat = 8
  /// 10: between a mark and its label, and a pill's inset.
  static let sPlus: CGFloat = 10
  /// 12: between a row's parts, and a row's vertical inset.
  static let m: CGFloat = 12
  /// 14: a row's side inset, and between a column's parts.
  static let mPlus: CGFloat = 14
  /// 16: a placeholder's inset and a bar's trailing inset.
  static let l: CGFloat = 16
  /// 20: around a side column of cards.
  static let xl: CGFloat = 20
  /// 24: between a column's sections, and a column's side inset.
  static let xxl: CGFloat = 24
  /// 32: around an alert.
  static let xxxl: CGFloat = 32
}
