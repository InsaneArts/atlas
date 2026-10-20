import CoreGraphics

/// Heights of controls and rows, in points.
enum ControlHeight {
  /// 20: a badge over a picture, such as Live.
  static let badge: CGFloat = 20
  /// 24: a status badge.
  static let pill: CGFloat = 24
  /// 26: a removable chip.
  static let chip: CGFloat = 26
  /// 28: a menu row.
  static let menuRow: CGFloat = 28
  /// 30: a control row among menu rows, a progress pill, and the small round button.
  static let row: CGFloat = 30
  /// 32: a compact fact row, and a chip floating over a picture.
  static let compactRow: CGFloat = 32
  /// 34: a capsule button and a round button.
  static let button: CGFloat = 34
  /// 40: a text field's row.
  static let field: CGFloat = 40
  /// 44: a large capsule button, and a regular fact or control row.
  static let large: CGFloat = 44
}
