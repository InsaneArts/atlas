import SwiftUI

extension View {
  /// Sets text in a style of the scale: its size and weight, its tracking, and the line spacing that makes each line
  /// the style's ``TextStyle/lineHeight``, so a view names its style once.
  ///
  /// - Parameters:
  ///   - style: The style.
  ///   - weight: The weight, or nil for the style's ``TextStyle/defaultWeight``.
  /// - Returns: The view's text in the style.
  func textStyle(_ style: TextStyle, weight: Font.Weight? = nil) -> some View {
    font(Typography.font(style, weight: weight ?? style.defaultWeight))
      .tracking(style.tracking)
      .lineSpacing(style.lineSpacing)
  }
}
