import CoreGraphics

/// Letter spacing, in points, for the few styles that set it.
enum LetterSpacing {
  /// −0.4: a headline, drawn in.
  static let headline: CGFloat = -0.4
  /// −0.13: a word on a top-row capsule or tab, drawn in by −0.01 em at 13 points.
  static let control: CGFloat = -0.13
  /// 0: the font's own, for words laid out at their widest whatever a capsule draws them in to.
  static let natural: CGFloat = 0
}
