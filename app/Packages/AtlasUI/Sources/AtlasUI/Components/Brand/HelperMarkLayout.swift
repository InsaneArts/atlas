import CoreGraphics

/// The helper mark's own measures.
enum HelperMarkLayout {
  /// About 22.5: the icon's tile in a row, the plate a session's app icon draws in its 28, so the helper stands the
  /// size of the apps listed above it.
  static let rowTile = SessionIcon.plate(SessionIcon.rowSide)
  /// 32: the icon's tile on the Helper page's card.
  static let cardTile = ControlHeight.m
  /// 8: the status dot.
  static let dot = Spacing.s
  /// 2: the ground's ring round the dot.
  static let ring = Spacing.half
  /// 12: the dot and its ring.
  static let reach = dot + ring * 2
}
