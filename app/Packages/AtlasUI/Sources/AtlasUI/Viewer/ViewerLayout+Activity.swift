import SwiftUI

/// The Activity outline's measures. Sessions, app runs and steps share one row whose highlight spans the list's width,
/// on the filters' and the header's leading edge, with its content 12 inside it on each side: an indent per level, a
/// disclosure column, a lead column, the text, an accessory and the time, which lines up at every level. A folded row
/// and its key items are one box. Pictures take one of three framed 16:10 sizes.
extension ViewerLayout {
  /// 8: above the line for the sessions waiting at the top of a scrolled-down list.
  static let waitingAbove = Spacing.s
  /// 24: that line's height.
  static let waitingHeight = ControlHeight.xs
  /// 12: that line's inset at its sides.
  static let waitingInset = Spacing.m
  /// 4: between its chevron and its words.
  static let waitingGap = Spacing.xs
  /// 8: its chevron's size.
  static let waitingChevron = Spacing.s
  /// 12: above and below the filters' track.
  static let filtersAround = Spacing.m

  /// 12: how far a row's content stands inside its highlight, on each side.
  static let outlineInset = Spacing.m
  /// 12: the disclosure's column, before every session's and run's lead.
  static let outlineDisclosure = IconSize.xs
  /// 8: between the disclosure and the lead.
  static let outlineDisclosureGap = Spacing.s
  /// 24: the lead's column, with its icon centred in it.
  static let outlineLead = IconSize.m
  /// 10: between the lead and the text.
  static let outlineLeadGap = Spacing.sPlus
  /// 26: each level's indent, so a child's disclosure stands centred under its parent's lead.
  static let outlineIndent = outlineDisclosure + outlineDisclosureGap + (outlineLead - outlineDisclosure) / 2
  /// 12: between the text or the accessory and the time.
  static let outlineTimeGap = Spacing.m
  /// 10: a row's highlight's corner, and a box's.
  static let outlineRowRadius = Radius.medium
  /// 4: between a box and what stands next to it, and between a box's row and its pictures.
  static let boxGap = Spacing.xs
  /// 12: under a box's last item.
  static let boxBelow = Spacing.m
  /// 44: a session's row, two lines.
  static let sessionRowHeight = ControlHeight.xl
  /// 32: a run's row.
  static let runRowHeight = ControlHeight.m
  /// 28: a step's row.
  static let stepRowHeight = ControlHeight.s
  /// 14: a step's glyph.
  static let stepGlyph = IconSize.xsPlus
  /// 1: between a session's two lines.
  static let sessionLineGap = Spacing.hairline
  /// 8: between a run's name and its summary.
  static let runSummaryGap = Spacing.s
  /// 4: between a run's summary and its problems.
  static let runProblemGap = Spacing.xs
  /// 6: a running run's dot, before its time.
  static let runningDot = IconSize.xxs
  /// 6: between the running dot and the time.
  static let runningDotGap = Spacing.xsPlus
  /// 6: between a step's words and its picture button.
  static let pictureButtonGap = Spacing.xsPlus
  /// 24 × 20: a step's picture button.
  static let pictureButton = CGSize(width: IconSize.m, height: Spacing.xl)
  /// 28: the picture button's hit area, each way.
  static let pictureButtonHit = ControlHeight.s
  /// 8: a link's chevron, after its words.
  static let countChevron: CGFloat = 8
  /// 4: between a link's words and its chevron.
  static let countChevronGap = Spacing.xs
  /// 24: a link's capsule.
  static let linkHeight = ControlHeight.xs
  /// 6: a link's side padding, so its words stand 6 inside its capsule.
  static let linkInset = Spacing.xsPlus
  /// 4: above and below the hairline between two sessions of a day, which runs from the text's edge to 12 short of the
  /// list's end.
  static let sessionSeparatorGap = Spacing.xs
  /// 8: under an opened run's last step.
  static let stepsBelow = Spacing.s
  /// 6: between two chords of keys.
  static let chordGap = Spacing.xsPlus
  /// 24: a day heading's height.
  static let dayHeight = ControlHeight.xs
  /// 12: above a day heading.
  static let dayAbove = Spacing.m
  /// 420: the card width at and under which Replay shows its symbol alone and one picture shows beside a link.
  static let outlineNarrowest: CGFloat = 420
  /// 560: the card width at and under which a run's summary hides and two pictures show beside a link.
  static let outlineNarrow: CGFloat = 560

  /// The text's leading edge at an outline level: 66 for the first level, then 26 more each level.
  ///
  /// - Parameter level: The level, 0 at the list's edge.
  /// - Returns: The edge, from the list's leading edge.
  static func outlineText(level: Int) -> CGFloat {
    outlineInset + CGFloat(level) * outlineIndent + outlineDisclosure + outlineDisclosureGap + outlineLead
      + outlineLeadGap
  }

  /// 80 × 50: a folded run's and an ended session's pictures.
  static let pictureFolded = CGSize(width: 80, height: 50)
  /// 128 × 80: a run's pictures opened in place.
  static let pictureGrid = CGSize(width: 128, height: 80)
  /// 176 × 110: a screenshot under its step.
  static let pictureInPlace = CGSize(width: 176, height: 110)
  /// 6: every picture frame's corner.
  static let pictureRadius = Radius.extraSmall
  /// 8: between two pictures, in a strip or a grid.
  static let pictureGap = Spacing.s
  /// 12: under a run's pictures opened in place.
  static let gridBelow = Spacing.m
  /// 4: between a grid and its links.
  static let gridLinksAbove = Spacing.xs
  /// 6: the most pictures a grid row holds.
  static let pictureGridColumns = 6
  /// 808: the widest a run's grid stands, six columns and their gaps.
  static let pictureGridWidth =
    pictureGrid.width * CGFloat(pictureGridColumns) + pictureGap * CGFloat(pictureGridColumns - 1)
  /// −2: how far a strip's link moves back toward the last picture, so its words start 12 after it.
  static let stripLinkLead = Spacing.m - pictureGap - linkInset
  /// 8: the pictures a run's grid shows before "Show N more".
  static let pictureGridFirst = 8
  /// 3: the most pictures a strip shows beside its link.
  static let stripMost = 3

  /// How many pictures a strip shows beside its link on a card this wide: three, two at 560 or narrower, one at 420 or
  /// narrower.
  ///
  /// - Parameter width: The card's width.
  /// - Returns: How many.
  static func stripCount(width: CGFloat) -> Int {
    if width <= outlineNarrowest { return 1 }
    return width <= outlineNarrow ? 2 : stripMost
  }

  /// 824: the picture sheet's width on a wide window, so its picture well is 800 × 500.
  static let sheetWidth: CGFloat = 824
  /// 26: the sheet's corner, an alert's.
  static let sheetRadius = Radius.extraLarge
  /// 12: the picture well's and the footer's inset from the sheet's edge.
  static let sheetInset = Spacing.m
  /// 16: the header's inset at the top and sides.
  static let sheetHeaderInset = Spacing.l
  /// 24: how far the picture stands inside its well.
  static let sheetPictureInset = Spacing.xxl
  /// 16: the sheet's least margin from the window's sides.
  static let sheetMargin = Spacing.l
  /// 4: between ‹ and ›.
  static let sheetArrowGap = Spacing.xs
  /// 46: where the sheet hangs from, under the card's header.
  static let sheetTop = cardInset + headerHeight
  /// 2000: how far an outline row's ground reaches below it, under the rows that follow, past any list's height, so
  /// what a disclosure brings in stays covered until the rows below have moved past it.
  static let outlineCurtain: CGFloat = 2000
  /// 0.97: how small the sheet starts as it comes, and ends as it goes, in its own place.
  static let sheetEnterScale: CGFloat = 0.97

  /// The sheet's width in a window this wide: 824, or the window's width less a margin each side.
  ///
  /// - Parameter window: The window's width.
  /// - Returns: The width.
  static func sheetWidth(window: CGFloat) -> CGFloat {
    max(min(sheetWidth, window - sheetMargin * 2), .zero)
  }
}
