import CoreGraphics

/// Heights of controls and rows, in points.
enum ControlHeight {
  /// 24: a status badge, and the mini screen's Live chip.
  static let pill: CGFloat = 24
  /// 26: a mini capsule, as Settings' buttons are.
  static let mini: CGFloat = 26
  /// 28: a menu row, a tab in a capsule track and a search field.
  static let menuRow: CGFloat = 28
  /// 30: a control row among menu rows, a progress pill, the small round button, a caption chip, and a small capsule:
  /// the panel's, an alert's and an empty message's.
  static let row: CGFloat = 30
  /// 32: a compact fact row, and the chip at the top of the screen.
  static let compactRow: CGFloat = 32
  /// 34: a capsule button and a round button.
  static let button: CGFloat = 34
  /// 40: a text field's row.
  static let field: CGFloat = 40
  /// 44: a large capsule button, and a regular fact or control row.
  static let large: CGFloat = 44
}
