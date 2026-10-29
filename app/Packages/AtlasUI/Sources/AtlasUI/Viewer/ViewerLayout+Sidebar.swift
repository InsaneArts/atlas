import SwiftUI

extension ViewerLayout {
  /// 236: the whole sidebar, its rows 216 across inside its 10-point sides: a session's 28-point icon, the 10 after
  /// it, and the rest for its name, which a longer one, such as "Claude Code · understudy", is cut short in its middle
  /// to fit, so its project still shows.
  static let sidebarWidth: CGFloat = 236
  /// 12: from the window's top to the sidebar's top bar, so the bar's centre is level with the window buttons'.
  static let sidebarTopInset = Spacing.m
  /// 10: the sidebar's inset at its sides and its foot, which its rows' capsules keep to.
  static let sidebarInset = Spacing.sPlus
  /// 28: the sidebar's top bar, the window buttons' place, the toggle and the search.
  static let topBarHeight = ControlHeight.s
  /// 14: under the top bar, above All sessions.
  static let topBarGap = Spacing.mPlus
  /// 4: between the sidebar toggle and the search.
  static let toggleGap = Spacing.xs
  /// 10: from the window buttons to the sidebar toggle beside them.
  static let lightsGap = Spacing.sPlus
  /// 28: the sidebar toggle and the search's circle.
  static let toggleSide = ControlHeight.s
  /// 8: between the sidebar toggle and the header's words while the sidebar is hidden.
  static let toggleClearance = Spacing.s
  /// 44: a session's row, its 28-point icon and 8 above and below it.
  static let sidebarRowHeight = ControlHeight.xl
  /// 32: a row of the Library.
  static let libraryRowHeight = ControlHeight.m
  /// 2: between the sidebar's rows.
  static let sidebarRowGap = Spacing.half
  /// 10: a row's content inset inside its capsule, before the icon and after the words or the count.
  static let sidebarRowInset = Spacing.sPlus
  /// 28: a host session's icon on its row.
  static let sidebarIcon = SessionIcon.rowSide
  /// 10: between a row's icon and its words.
  static let sidebarIconGap = Spacing.sPlus
  /// 1: between a session's name and its line under it.
  static let sidebarLineGap = Spacing.hairline
  /// 22: each of the icons All sessions stacks.
  static let stackIcon: CGFloat = 22
  /// 4: how far each icon behind the first peeks out past the one in front of it, so the first stands whole and the
  /// others show only their edge, as a deck of cards does.
  static let stackStep = Spacing.xs
  /// 2: the soft shadow's blur each stacked icon casts on the one behind it.
  static let stackShadowRadius: CGFloat = 2
  /// 1: how far that shadow falls towards the icons behind.
  static let stackShadowOffset = Spacing.hairline
  /// The room All sessions' icons take: a session's icon's, or the stack's reach where three stand wider.
  ///
  /// - Parameter count: How many icons stand in the stack.
  /// - Returns: The width.
  static func stackWidth(count: Int) -> CGFloat {
    max(sidebarIcon, stackIcon + stackStep * CGFloat(max(count - 1, 0)))
  }
  /// 5: how far a badge's frame reaches past its icon's bottom and trailing edges, so its dot stands 3 past them, its
  /// 2-point ring the rest.
  static let badgeReach: CGFloat = 5
  /// 11: the spinner in a badge's place while a session's work is cleaned up, the badge's own side.
  static let badgeSpinner: CGFloat = 11
  /// 2: the ground's ring round that spinner, as round a badge.
  static let badgeRing = Stroke.thick
  /// A heading's inset: 10 at its sides, in line with the rows' icons, 12 above it and 6 above the rows under it.
  static let headingInset = EdgeInsets(
    top: Spacing.m,
    leading: Spacing.sPlus,
    bottom: Spacing.xsPlus,
    trailing: Spacing.sPlus
  )
  /// 12: between the sessions' room and the Library under it, the gap a section's heading keeps above it.
  static let footGap = headingInset.top
  /// 16: how far the sessions fade out at their room's lower edge; their last row ends that far up, so scrolled to the
  /// end it stands clear of the fade.
  static let sessionsFade = Spacing.l
  /// 6: between Ended's chevron and its word.
  static let headingChevronGap = Spacing.xsPlus
  /// 10: the room Ended's chevron takes.
  static let headingChevron = Spacing.sPlus
  /// 90: how far Ended's chevron turns as Ended opens, from pointing on to pointing down.
  static let chevronTurn: Double = 90
  /// 24: Ended's heading as a button, its fill a capsule round the line.
  static let endedHeadingHeight = ControlHeight.xs
  /// 12: the corner of Ended's fill, half its height.
  static let endedHeadingCorner = endedHeadingHeight / 2
  /// Ended's inset round its button: 6 above and 2 under, so its line stands where another heading's does.
  static let endedHeadingInset = EdgeInsets(top: Spacing.xsPlus, leading: .zero, bottom: Spacing.half, trailing: .zero)
  /// 18: the room a Library row's symbol takes, so the three words start in line.
  static let librarySymbol = IconSize.sPlus
  /// 8: above and below the helper's line.
  static let helperInset = Spacing.s
  /// 8: the helper's dot.
  static let helperDot = Spacing.s
  /// 8: between the helper's dot and its words.
  static let helperGap = Spacing.s
  /// 16: the corner of the helper line's fill under the pointer, half the most it stands tall, so the fill is a
  /// capsule round the line.
  static let helperCorner = ControlHeight.m / 2
  /// 6: how far a row of Ended rises from as it comes in, from under its heading, and sinks to as it goes.
  static let endedRise = Spacing.xsPlus
  /// 12: how far past a threshold the window must go to cross back, so a window held near one never flickers between
  /// its two layouts.
  static let fitRoom = Spacing.m
  /// 700: the window's width from which the sidebar shows beside the card, hiding only under 688. The sidebar's 236
  /// leaves the card 456 at 700 and 444 at 688, room for the footer's widest buttons beside a latest line of about
  /// 120, so the card is never cramped beside it.
  static let wholeSidebar = ViewerThreshold(risesAt: 700, room: fitRoom)
  /// 420: the window's height under which the Library folds into a row of circles, opening again only at 432.
  static let tallSidebar = ViewerThreshold(fallsUnder: 420, room: fitRoom)
  /// 8: above and below the Library folded into a row, so the row stands 44 high.
  static let libraryRowInset = Spacing.s
  /// 4: between the circles of the Library folded into a row.
  static let libraryCircleGap = Spacing.xs
  /// 28: each circle of the Library folded into a row, and the helper's dot's at its end.
  static let libraryCircle = ControlHeight.s
  /// The Library's unseen dot's inset in a circle's top-trailing corner, where a badge stands.
  static let libraryCircleDot = Spacing.xs

  /// The room the sidebar takes at the window's leading edge: its width, or none while hidden. Its content keeps its
  /// width as it hides, so its rows are cut off by its narrowing edge rather than laid out again.
  ///
  /// - Parameter mode: How the sidebar stands.
  /// - Returns: The room.
  static func sidebarRoom(_ mode: ViewerSidebarMode) -> CGFloat {
    mode == .hidden ? .zero : sidebarWidth
  }

  /// The card's inset from the sidebar's edge, or from the window's while the sidebar is hidden: the sidebar's own
  /// inset keeps the two apart, so the card meets it; alone, it keeps the card's inset from the window's edge.
  ///
  /// - Parameter mode: How the sidebar stands.
  /// - Returns: The card's leading inset.
  static func cardLeading(_ mode: ViewerSidebarMode) -> CGFloat {
    mode == .hidden ? cardInset : .zero
  }

  /// 216: a row's capsule across, the sidebar's width less its sides.
  static let pillWidth = sidebarWidth - sidebarInset * 2

  /// 196: a row's content across inside its capsule's insets.
  static let rowContentWidth = pillWidth - sidebarRowInset * 2

  /// Where the sidebar toggle stands, in the window, as its top-leading corner: beside the window buttons, or where
  /// they would be in full screen, which hides them. It stays there while the sidebar hides, so hiding and showing
  /// never move it.
  ///
  /// - Parameter isFullScreen: Whether the window is in full screen.
  /// - Returns: The toggle's origin.
  static func toggleOrigin(isFullScreen: Bool) -> CGPoint {
    let x = isFullScreen ? lightsInset : lightsInset + lightsWidth + lightsGap
    return CGPoint(x: x, y: sidebarTopInset)
  }

  /// How far into the top bar the search may open: past the window buttons and the toggle.
  ///
  /// - Parameter isFullScreen: Whether the window is in full screen.
  /// - Returns: The search's leading room, from the sidebar's inset.
  static func searchLeading(isFullScreen: Bool) -> CGFloat {
    toggleOrigin(isFullScreen: isFullScreen).x + toggleSide + toggleGap - sidebarInset
  }

  /// The header's inset before its words: its own beside the sidebar; while the sidebar is hidden, enough to clear
  /// the window buttons and the toggle that stay in the card's corner, 110 in a window and 48 in full screen.
  ///
  /// - Parameters:
  ///   - mode: How the sidebar stands.
  ///   - isFullScreen: Whether the window is in full screen.
  /// - Returns: The header's leading inset.
  static func headerLeading(mode: ViewerSidebarMode, isFullScreen: Bool) -> CGFloat {
    guard mode == .hidden else { return headerInset }
    let toggle = toggleOrigin(isFullScreen: isFullScreen)
    return toggle.x + toggleSide + toggleClearance - cardInset
  }
}
