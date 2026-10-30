import SwiftUI

/// The body's own measures.
enum CapsuleButtonBodyLayout {
  /// 2: how far outside the outline the focus ring sits, so it reads as a ring rather than an edge.
  static let focusGap = Spacing.half
  /// 0.38: a circle's symbol against its side, so it sits with room around it at either size.
  static let symbolScale: CGFloat = 0.38
}
