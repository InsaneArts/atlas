import SwiftUI

/// Builds fonts from the type scale. A view says `.font(Typography.font(.body, weight: .semibold))`, never a size.
enum Typography {
  /// A system font from the scale.
  ///
  /// - Parameters:
  ///   - style: The size.
  ///   - weight: The weight, or nil for the system's regular.
  ///   - design: The face, such as `.monospaced` for a terminal's `>_`, or nil for the system's.
  /// - Returns: The system font at the style's size.
  static func font(_ style: TextStyle, weight: Font.Weight? = nil, design: Font.Design? = nil) -> Font {
    .system(size: style.pointSize, weight: weight, design: design)
  }
}
