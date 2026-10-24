import CoreGraphics

/// Gaps and insets, on a 2-point grid. A value only one window uses lives in that window's layout; a value off the
/// grid that a component cannot derive is a private constant in the component, with its reason.
enum Spacing {
  /// 1: a hairline, the one step off the grid.
  static let hairline: CGFloat = 1
  /// 2: between a title and its detail.
  static let xxs: CGFloat = 2
  /// 4: between a headline and its reason, and a lede's line spacing.
  static let xs: CGFloat = 4
  /// 6: between page dots, and between a dot and its label.
  static let s: CGFloat = 6
  /// 8: between buttons in a row and inside a pill.
  static let m: CGFloat = 8
  /// 10: between a mark and its label, and a pill's inset.
  static let ml: CGFloat = 10
  /// 12: between a row's parts, and a row's vertical inset.
  static let l: CGFloat = 12
  /// 14: a row's side inset, and between a column's parts.
  static let xl: CGFloat = 14
  /// 16: a placeholder's inset and a bar's trailing inset.
  static let xxl: CGFloat = 16
  /// 20: around a side column of cards.
  static let group: CGFloat = 20
  /// 24: between a column's sections, and a column's side inset.
  static let section: CGFloat = 24
  /// 32: around an alert.
  static let page: CGFloat = 32
}
