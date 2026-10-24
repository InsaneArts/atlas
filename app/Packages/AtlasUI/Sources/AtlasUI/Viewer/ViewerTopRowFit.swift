/// How far the top row draws in to fit a narrowing window, one step after another, as the widget's does. Each step
/// keeps every earlier one.
enum ViewerTopRowFit: Int, Comparable {
  /// Everything at its full size.
  case regular
  /// Keys keeps its glyph and drops its word.
  case keysGlyph
  /// The tabs draw their words in closer.
  case tightTabs
  /// The status keeps its word and drops the line beside it.
  case statusWord
  /// The white capsule says its short word, such as Take for Take Control.
  case shortWords

  /// Whether this step is past another.
  ///
  /// - Parameters:
  ///   - lhs: One step.
  ///   - rhs: The other.
  /// - Returns: Whether `lhs` comes before `rhs`.
  static func < (lhs: ViewerTopRowFit, rhs: ViewerTopRowFit) -> Bool {
    lhs.rawValue < rhs.rawValue
  }
}
