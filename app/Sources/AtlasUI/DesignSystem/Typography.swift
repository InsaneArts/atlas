import SwiftUI

/// Builds fonts from the type scale. A view says `.font(Typography.font(.body, weight: .semibold))`, never a size.
enum Typography {
  /// 0.75: the least a headline shrinks to keep within its lines, as a long account name needs, so it still reads as
  /// a headline rather than running on or being cut short.
  static let leastHeadlineScale: CGFloat = 0.75
  /// 2: the most lines a headline's statement or its tail takes; past them it shrinks.
  static let headlineLines = 2

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
