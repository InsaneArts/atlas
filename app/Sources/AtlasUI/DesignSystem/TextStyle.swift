import CoreGraphics

/// The type scale, largest first. Weights stay with each use; a view names a style, never a size. Three steps are
/// marked [R]: their sizes are today's exact ones, kept until the rounding phase (3b) decides each.
enum TextStyle: CaseIterable, Sendable {
  /// 27: a screen's headline. [R] Kept at 27 until Phase 3b rounds it to 28.
  case largeTitle
  /// 24: a headline over a picture, such as an alert's.
  case title
  /// 22: a narrow panel's headline.
  case title2
  /// 20: a column's title, such as an account's name, and a large avatar's letter.
  case title3
  /// 14: a window's title, a large button and a tile's symbol.
  case headline
  /// 13: running text, row titles, fields, buttons and menu rows.
  case body
  /// 12.5: a chip, a compact card's title and a work note. [R] Kept at 12.5 until Phase 3b rounds it to 12
  /// (callout).
  case label
  /// 12: details, facts, hints and pills.
  case callout
  /// 11.5: small print, such as a note, a reason under a title and an action's time. [R] Kept at 11.5 until
  /// Phase 3b rounds it to 11 (caption).
  case note
  /// 11: an eyebrow, and the letter in a disc.
  case caption
  /// 10: a badge over a picture, and a chip's symbol.
  case caption2
  /// 9: the smallest glyph, such as a chip's remove button.
  case footnote

  /// The style's size in points.
  var pointSize: CGFloat {
    switch self {
    case .largeTitle: 27
    case .title: 24
    case .title2: 22
    case .title3: 20
    case .headline: 14
    case .body: 13
    case .label: 12.5
    case .callout: 12
    case .note: 11.5
    case .caption: 11
    case .caption2: 10
    case .footnote: 9
    }
  }
}
