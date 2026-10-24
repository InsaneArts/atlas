import CoreGraphics

/// The type scale, largest first. Weights stay with each use; a view names a style, never a size.
enum TextStyle: CaseIterable, Sendable {
  /// 28: a screen's headline.
  case largeTitle
  /// 24: a headline over a picture, such as an alert's.
  case title
  /// 22: a narrow panel's headline.
  case title2
  /// 20: a column's title, such as an account's name, and a large avatar's letter.
  case title3
  /// 15: a pane's title and an empty message's headline.
  case heading
  /// 14: a window's title, a large button and a tile's symbol.
  case headline
  /// 13: running text, row titles, fields, buttons and menu rows.
  case body
  /// 12: details, facts, hints and pills; a chip, a compact card's title and a work note.
  case callout
  /// 11: small print, such as a note, a reason under a title and an action's time; a group's label, and the letter
  /// in a disc.
  case caption
  /// 10: a badge over a picture, and a chip's symbol.
  case caption2
  /// 9: the smallest glyph, such as a popup button's chevron and a disc's mark.
  case footnote

  /// The style's size in points.
  var pointSize: CGFloat {
    switch self {
    case .largeTitle: 28
    case .title: 24
    case .title2: 22
    case .title3: 20
    case .heading: 15
    case .headline: 14
    case .body: 13
    case .callout: 12
    case .caption: 11
    case .caption2: 10
    case .footnote: 9
    }
  }
}
