import SwiftUI

extension ViewerLayout {
  /// 8: the content card's inset from the window's top, trailing and bottom edges, and from the sidebar.
  static let cardInset = Spacing.s
  /// 8: the content card's corner, the window's less its inset, so the two curves run side by side.
  static let cardRadius = Radius.nested(in: Radius.window, inset: cardInset)
  /// 38: the card's header, one line of who has the helper; its centre sits level with the window buttons'.
  static let headerHeight: CGFloat = 38
  /// 14: the header's inset at its sides.
  static let headerInset = Spacing.mPlus
  /// 8: between who has the helper and what they do with it.
  static let headerGap = Spacing.s
  /// 46: the card's footer, the latest action and the buttons.
  static let footerHeight: CGFloat = 46
  /// 14: the footer's inset before its line, in line with the header's words.
  static let footerLeading = Spacing.mPlus
  /// 10: the footer's inset after its buttons, so the main capsule's curve sits in the card's corner.
  static let footerTrailing = Spacing.sPlus
  /// 6: between the footer's buttons, and between its line and them.
  static let footerGap = Spacing.xsPlus
  /// 6: between the footer line's icon or mark and its words.
  static let footerLineGap = Spacing.xsPlus
  /// 16: the session's icon before the footer's line.
  static let footerIcon = IconSize.s
  /// 6: between a footer capsule's symbol and its title.
  static let footerSymbolGap = Spacing.xsPlus
  /// The stage's inset inside the card: none under the header, which already leaves room, and 10 at the sides and
  /// above the footer.
  static let stageInset = EdgeInsets(top: .zero, leading: Spacing.sPlus, bottom: Spacing.sPlus, trailing: Spacing.sPlus)
  /// 6: the screen's corner on the stage.
  static let screenRadius = Radius.extraSmall
  /// 3: control's accent edge, drawn inside the screen's shape.
  static let controlEdge: CGFloat = 3
  /// 36: the row over the Activity list that holds its filters.
  static let filtersHeight: CGFloat = 36
  /// 200: the small screen's width over a list, in a card body at least ``pipFullBody`` wide.
  static let pipWidth: CGFloat = 200
  /// 140: the narrowest the small screen draws, in a narrow card.
  static let pipNarrowest: CGFloat = 140
  /// 560: the card body's width from which the small screen draws at ``pipWidth``; narrower, it shrinks with the body.
  static let pipFullBody: CGFloat = 560
  /// 16:10: the small screen's shape before there is a picture to take it from.
  static let pipAspect: CGFloat = 16.0 / 10.0
  /// 4:3 to 16:9: the shapes the small screen takes from the picture, so an odd display never draws it a sliver.
  static let pipAspects: ClosedRange<CGFloat> = (4.0 / 3.0)...(16.0 / 9.0)
  /// 9: the small screen's corner, which the stage's picture rounds to as it shrinks into it.
  static let pipRadius: CGFloat = 9
  /// 12: the small screen's inset from the card body's edges, so it sits clear of the footer.
  static let pipInset = Spacing.m
  /// 12: how far a list coming from the Screen rises into place, and sinks as it goes back to it.
  static let listRise = Spacing.m
  /// 420: the card's width from which the footer's capsules keep their words, given up only under 408: its buttons'
  /// widest set, about 250, its insets and a latest line of about 120.
  static let footerWords = ViewerThreshold(risesAt: 420, room: fitRoom)
  /// 260: the card's width under which the footer's line keeps only its session's icon and its age, its words coming
  /// back only at 272.
  static let footerLine = ViewerThreshold(fallsUnder: 260, room: fitRoom)
  /// 360: the card's width under which the header gives up what its holder does, coming back only at 372.
  static let headerWhat = ViewerThreshold(fallsUnder: 360, room: fitRoom)

  /// The card's width in a window `windowWidth` wide beside the sidebar as it stands: the window's less the sidebar's
  /// room and the card's insets.
  ///
  /// - Parameters:
  ///   - windowWidth: The window's width.
  ///   - mode: How the sidebar stands.
  /// - Returns: The card's width; never less than nothing.
  static func cardWidth(windowWidth: CGFloat, mode: ViewerSidebarMode) -> CGFloat {
    max(windowWidth - sidebarRoom(mode) - cardLeading(mode) - cardInset, .zero)
  }

  /// The small screen's size over a list: ``pipWidth`` across in a card body at least ``pipFullBody`` wide, shrinking
  /// with a narrower body but never under ``pipNarrowest``, at the picture's own shape within ``pipAspects``, so the
  /// picture fills it with no bars at its sides.
  ///
  /// - Parameters:
  ///   - bodyWidth: The card body's width, as the lists are laid out at.
  ///   - aspect: The picture's width over its height; nil before there is one.
  /// - Returns: Its size, in whole points.
  static func pipSize(bodyWidth: CGFloat, aspect: CGFloat?) -> CGSize {
    let width = min(max(pipWidth * bodyWidth / pipFullBody, pipNarrowest), pipWidth)
      .rounded()
    let shape = aspect.flatMap { $0.isFinite && $0 > 0 ? $0 : nil } ?? pipAspect
    let fitted = min(max(shape, pipAspects.lowerBound), pipAspects.upperBound)
    return CGSize(width: width, height: (width / fitted).rounded())
  }

  /// The room a list's scrolling content keeps at the end the small screen floats over: its height and its inset on
  /// both sides of it, so the last row, or the first, can be scrolled wholly clear of it.
  ///
  /// - Parameter pip: The small screen's size.
  /// - Returns: The room.
  static func pipRoom(_ pip: CGSize) -> CGFloat {
    pip.height + pipInset * 2
  }

  /// 8: how far past a selected row its room reaches on every side, so a step shows its ring whole too.
  static let listRevealMargin = Spacing.s
}
