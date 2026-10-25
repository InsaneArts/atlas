import CoreGraphics

/// The proportions shared by brand glyphs and their small cards.
enum BrandMark {
  /// One half: the glyph's share of its card's side.
  static let glyphFraction: CGFloat = 0.5
  /// One point, lifting the icon gently at row sizes.
  static let shadowRadius: CGFloat = 1
}
