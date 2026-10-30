import SwiftUI

/// The Apps pane's measures: its summary, its groups' headings, its grid of cards and what each card holds. A card's
/// corner is its picture's plus the card's padding, so the two curves run side by side.
extension ViewerLayout {
  /// 14: above the summary's title.
  static let appsSummaryTop = Spacing.mPlus
  /// 6: under the summary, above the first group.
  static let appsSummaryBottom = Spacing.xsPlus
  /// 4: between the summary's title and how many windows there are.
  static let appsSummaryGap = Spacing.xs
  /// 14: above a group's heading.
  static let appGroupAbove = Spacing.mPlus
  /// 6: under a group's heading, above its cards.
  static let appGroupBelow = Spacing.xsPlus
  /// 24: a group's heading.
  static let appGroupHeight = ControlHeight.xs
  /// 232: the least width a card takes before the grid drops a column.
  static let appCardLeast: CGFloat = 232
  /// 4: the most cards across, however wide the pane.
  static let appCardColumns = 4
  /// 12: between cards, across and down.
  static let appCardGap = Spacing.m
  /// 12: a card's padding, all round.
  static let appCardPadding = Spacing.m
  /// 10: between the parts of a card: its head, its picture, its tags and its windows.
  static let appCardParts = Spacing.sPlus
  /// 8: the corner of a card's picture.
  static let appCardPictureRadius = Radius.small
  /// 20: a card's corner, its picture's and its padding, so they are concentric.
  static let appCardRadius = appCardPictureRadius + appCardPadding
  /// 44: an app's icon at the head of its card.
  static let appCardIcon = IconSize.xlPlus
  /// 12: between a card's icon and its name.
  static let appCardIconGap = Spacing.m
  /// 2: between a card's name and the line under it.
  static let appCardNameGap = Spacing.half
  /// 14: the icon of the session that opened an app, inline before its name.
  static let appCardSessionIcon = IconSize.xsPlus
  /// 4: between that icon and the session's name.
  static let appCardSessionGap = Spacing.xs
  /// 96: a card's picture's height.
  static let appCardPicture: CGFloat = 96
  /// 6: between a card's tags.
  static let appCardTagGap = Spacing.xsPlus
  /// 28: a window's row on its card, a true capsule.
  static let appCardWindowHeight = ControlHeight.s
  /// 8: a window row's inset at its ends.
  static let appCardWindowInset = Spacing.s
  /// 8: between a window row's glyph and its title.
  static let appCardWindowGlyphGap = Spacing.s
  /// 2: between a card's window rows.
  static let appCardWindowGap = Spacing.half
  /// 3: the most windows a card lists before the rest go into "N more".
  static let appCardWindowsShown = 3
  /// 10: the map's inset in a card's picture, all round.
  static let appMapInset = Spacing.sPlus
  /// 4: the corner of the display drawn small in a map.
  static let appMapRadius: CGFloat = 4
  /// 2: the corner of a window drawn in a map.
  static let appMapWindowRadius: CGFloat = 2
}
