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

  /// A system font for a glyph sized from its frame rather than from the scale, such as a symbol at a fraction of the
  /// circle it sits in. Only for glyphs sized from a frame: the size is arithmetic on the frame's token, never a
  /// literal, which the token checker refuses.
  ///
  /// - Parameters:
  ///   - size: The glyph's size in points, derived from its frame.
  ///   - weight: The weight, or nil for the system's regular.
  /// - Returns: The system font at that size.
  static func font(size: CGFloat, weight: Font.Weight? = nil) -> Font {
    .system(size: size, weight: weight)
  }
}
