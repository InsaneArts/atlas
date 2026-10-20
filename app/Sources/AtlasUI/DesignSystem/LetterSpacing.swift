import CoreGraphics

/// Letter spacing, in points, for the few styles that set it.
enum LetterSpacing {
  /// 0.9: an eyebrow's uppercase letters, opened up.
  static let eyebrow: CGFloat = 0.9
  /// −0.4: a headline, drawn in.
  static let headline: CGFloat = -0.4
  /// −0.3: a smaller title, such as an account's name over its reason.
  static let title: CGFloat = -0.3
}
