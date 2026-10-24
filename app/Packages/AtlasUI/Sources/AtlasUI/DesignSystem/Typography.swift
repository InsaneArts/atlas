import SwiftUI

/// Builds fonts from the type scale. A view says `.font(Typography.font(.body, weight: .semibold))`, never a size.
enum Typography {
  /// 2: the most lines a headline's statement or its tail takes; past them it is cut in the middle.
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

  /// The system font from the scale with tabular digits, so numbers that change keep their width: Activity's
  /// times, sizes, versions and Recent's times. The system face's tabular digits keep numeric columns aligned
  /// without introducing a second typeface.
  ///
  /// - Parameters:
  ///   - style: The size.
  ///   - weight: The weight, or nil for the style's ``TextStyle/defaultWeight``.
  /// - Returns: The system font at the style's size, its digits all one width.
  static func digits(_ style: TextStyle, weight: Font.Weight? = nil) -> Font {
    font(style, weight: weight ?? style.defaultWeight)
      .monospacedDigit()
  }

  /// The font an SF Symbol takes in its role, at the weight of the text beside it.
  ///
  /// - Parameters:
  ///   - scale: The symbol's role, such as ``SymbolScale/control``.
  ///   - weight: The weight of the text it sits with, or nil for the system's regular.
  /// - Returns: The system font at the role's size.
  static func symbol(_ scale: SymbolScale, weight: Font.Weight? = nil) -> Font {
    .system(size: scale.pointSize, weight: weight)
  }

  /// A system font for a glyph sized from its frame rather than from the scale, such as a symbol at a fraction of the
  /// circle it sits in, or for words at a size between the scale's steps that one component's design asks for, such
  /// as the panel's small chip at 11.5. The size is arithmetic on a token or a named measure of that component, never
  /// a literal, which the token checker refuses.
  ///
  /// - Parameters:
  ///   - size: The glyph's size in points, derived from its frame.
  ///   - weight: The weight, or nil for the system's regular.
  /// - Returns: The system font at that size.
  static func font(size: CGFloat, weight: Font.Weight? = nil) -> Font {
    .system(size: size, weight: weight)
  }
}
