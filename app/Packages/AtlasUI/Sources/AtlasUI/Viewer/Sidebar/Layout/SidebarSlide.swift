import CoreGraphics

/// How the sidebar stands as it moves, for the card beside it: the mode drawn now, which the card's edge and its
/// header's inset follow on the slide's spring; the mode it settles on, which the lists are laid out for at once; and
/// the mode it last came to rest in. A line of words lays out at the narrowest room any of them gives it, so its words
/// are cut short once, as the card narrows, and only take more room once the card has widened, never changing while
/// the card's edge travels, when they would be drawn halfway between two layouts.
struct SidebarSlide: Equatable {
  /// The mode drawn now.
  var drawn: ViewerSidebarMode
  /// The mode the sidebar settles on.
  var settling: ViewerSidebarMode
  /// The mode the sidebar last came to rest in.
  var rested: ViewerSidebarMode

  /// The sidebar at rest in `mode`.
  ///
  /// - Parameter mode: How it stands.
  init(_ mode: ViewerSidebarMode) {
    self.init(drawn: mode, settling: mode, rested: mode)
  }

  /// The sidebar as it moves.
  ///
  /// - Parameters:
  ///   - drawn: The mode drawn now.
  ///   - settling: The mode it settles on.
  ///   - rested: The mode it last came to rest in.
  init(drawn: ViewerSidebarMode, settling: ViewerSidebarMode, rested: ViewerSidebarMode) {
    self.drawn = drawn
    self.settling = settling
    self.rested = rested
  }

  /// How much less room than the card gives it now a line of words takes: as much as the card, less `leading` before
  /// the words, is wider in the mode drawn than in the narrowest of the three modes; none at rest.
  ///
  /// - Parameter leading: The inset before the line's words in a mode, which the line's room loses.
  /// - Returns: The room held back, at the line's trailing end.
  func wordsHold(leading: (ViewerSidebarMode) -> CGFloat) -> CGFloat {
    let most =
      [drawn, settling, rested]
      .map { Self.taken($0, leading: leading) }
      .max() ?? .zero
    return most - Self.taken(drawn, leading: leading)
  }

  /// How much of the window's width the sidebar and the line's leading inset take from a line of words in the card
  /// in `mode`: the card's width in a mode is the window's less what the sidebar takes, so only that part differs.
  ///
  /// - Parameters:
  ///   - mode: How the sidebar stands.
  ///   - leading: The inset before the line's words in a mode.
  /// - Returns: The width taken.
  private static func taken(_ mode: ViewerSidebarMode, leading: (ViewerSidebarMode) -> CGFloat) -> CGFloat {
    ViewerLayout.sidebarRoom(mode) + ViewerLayout.cardLeading(mode) + leading(mode)
  }
}
