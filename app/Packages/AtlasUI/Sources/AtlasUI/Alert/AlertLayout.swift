import CoreGraphics

/// The alert window's own measures, which no other window shares.
enum AlertLayout {
  /// 64: the room around the alert for its shadow, which the window draws inside its own clear frame: the alert's 24
  /// fall plus most of its 30 blur, so the shadow fades out before the window's edge.
  static let shadowRoom: CGFloat = 64
}
