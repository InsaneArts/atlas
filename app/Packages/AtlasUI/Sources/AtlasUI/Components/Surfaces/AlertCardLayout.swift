import SwiftUI

/// The alert card's own measures, at file scope because `AlertCard` is generic.
enum AlertCardLayout {
  /// 260: the alert's width.
  static let width: CGFloat = 260
  /// 52: the icon's tile; its canvas draws larger, past the frame.
  static let icon: CGFloat = 52
  /// 10: the longest label that still sits side by side.
  static let shortLabel = 10
  /// 7: between a button's mark and its word.
  static let markGap: CGFloat = 7
  /// The top highlight: the surface's edge again, faded out by a fifth of the way down, where the corners end.
  static var highlight: LinearGradient {
    LinearGradient(
      colors: [Color.strokeStrong, .clear],
      startPoint: .top,
      endPoint: UnitPoint(x: 0.5, y: 0.2)
    )
  }
}
