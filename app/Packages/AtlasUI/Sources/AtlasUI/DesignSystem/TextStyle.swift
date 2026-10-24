import SwiftUI

/// The type scale, largest first. A view names a style, never a size: through `View.textStyle(_:weight:)`,
/// which sets the style's size, weight, tracking and line height at once, or through `Typography.font`, where the
/// weight stays with the use.
enum TextStyle: CaseIterable, Sendable {
  /// 28: a screen's headline.
  case largeTitle
  /// 24: a headline over a picture, such as an alert's, and a setup screen's title (v3 D1: 24/29 semibold −0.36).
  case title
  /// 22: a narrow panel's headline.
  case title2
  /// 20: a column's title, such as an account's name, and a large avatar's letter; a title too long for ``title``
  /// (v3 D1: 20/25).
  case title3
  /// 17: the title of an empty state or of a stage waiting on something, such as Not ready or Connecting (v3
  /// `emptyTitle`, D1: 17/22 semibold −0.2, named here by its place in the scale).
  case subheading
  /// 15: a pane's title and an empty message's headline (v3 `paneTitle`, D1: 15/20 semibold −0.15).
  case heading
  /// 14: a window's title, a large button and a tile's symbol.
  case headline
  /// 13: running text, row titles, fields, buttons and menu rows (v3 D1: 13/17).
  case body
  /// 12: details, facts, hints and pills; a chip, a compact card's title and a work note (v3 D1: 12/16).
  case callout
  /// 11: small print, such as a note, a reason under a title and an action's time; a group's label, and the letter
  /// in a disc (v3 D1: 11/14).
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
    case .subheading: 17
    case .heading: 15
    case .headline: 14
    case .body: 13
    case .callout: 12
    case .caption: 11
    case .caption2: 10
    case .footnote: 9
    }
  }

  /// The height of one line in points, as v3's D1 sets it, or nil for a style that keeps the font's own.
  var lineHeight: CGFloat? {
    switch self {
    case .title: 29
    case .title3: 25
    case .subheading: 22
    case .heading: 20
    case .body: 17
    case .callout: 16
    case .caption: 14
    case .largeTitle, .title2, .headline, .caption2, .footnote: nil
    }
  }

  /// The space added between lines so each line is ``lineHeight``. SwiftUI's line spacing adds to the font's own line
  /// height, so this is v3's line height less the system font's default line height at the style's size and default
  /// weight, as `NSLayoutManager.defaultLineHeight(for:)` measured it on macOS 27: 24 → 28, 20 → 23, 17 → 20,
  /// 15 → 18, 13 → 16, 12 → 15, 11 → 13. Zero for a style that keeps the font's own height.
  var lineSpacing: CGFloat {
    switch self {
    case .title: 1
    case .title3: 2
    case .subheading: 2
    case .heading: 2
    case .body: 1
    case .callout: 1
    case .caption: 1
    case .largeTitle, .title2, .headline, .caption2, .footnote: 0
    }
  }

  /// The weight a style takes when its use names none (U-4): titles and headings semibold, text regular. A row's
  /// medium title (v3 D1) is its row's choice, not the style's.
  var defaultWeight: Font.Weight {
    switch self {
    case .largeTitle, .title, .title2, .title3, .subheading, .heading, .headline: .semibold
    case .body, .callout, .caption, .caption2, .footnote: .regular
    }
  }

  /// The letter spacing in points, drawn in for the larger titles (v3 D1), none for the rest.
  var tracking: CGFloat {
    switch self {
    case .title: -0.36
    case .subheading: -0.2
    case .heading: -0.15
    case .largeTitle, .title2, .title3, .headline, .body, .callout, .caption, .caption2, .footnote: 0
    }
  }
}
