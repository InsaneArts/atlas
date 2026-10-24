import CoreGraphics

/// Heights of controls and rows, in points, named by size as ``Spacing`` is: a `Plus` step is 2 points above its
/// named step.
enum ControlHeight {
  /// 24: a status badge, and the mini screen's Live chip.
  static let xs: CGFloat = 24
  /// 26: a mini capsule, as Settings' buttons are.
  static let xsPlus: CGFloat = 26
  /// 28: a menu row, a tab in a capsule track and a search field.
  static let s: CGFloat = 28
  /// 30: a control row among menu rows, a progress pill, the small round button, a caption chip, and a small capsule:
  /// the panel's, an alert's and an empty message's.
  static let sPlus: CGFloat = 30
  /// 32: a compact fact row, and the chip at the top of the screen.
  static let m: CGFloat = 32
  /// 34: a capsule button and a round button.
  static let mPlus: CGFloat = 34
  /// 40: a text field's row.
  static let l: CGFloat = 40
  /// 44: a large capsule button, and a regular fact or control row.
  static let xl: CGFloat = 44
}
