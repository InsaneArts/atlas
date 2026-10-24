import CoreGraphics

/// Letter spacing, in points, for the few styles that set it.
enum LetterSpacing {
  /// −0.4: a headline, drawn in.
  static let headline: CGFloat = -0.4
  /// −0.13: a word on a top-row capsule or tab, drawn in by the boards' −0.01 em at 13 points (v3.py `pill`,
  /// `segmented`).
  static let control: CGFloat = -0.13
}
