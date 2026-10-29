import CoreGraphics

/// How the main window's content gives way to the window's size, as its controller decides it from each resize and
/// the views draw it: the sidebar beside the card or hidden by the width; the footer's capsules turning into circles
/// and its line giving up its words as the card narrows; the header giving up what its holder does; and the Library
/// folding into a row of circles in a short window. Each part changes only once its threshold is crossed by its room
/// (``ViewerThreshold``), so a window held near one never flickers and a resize changes the content only at those
/// steps.
public struct ViewerFit: Equatable, Sendable {
  /// How the width leaves the sidebar: beside the card or hidden; the person's own hiding goes over it.
  public var sidebar: ViewerSidebarMode
  /// How the footer stands.
  public var footer: ViewerFooterFit
  /// Whether the header keeps what its holder does after them, such as "has the helper".
  public var showsWhat: Bool
  /// Whether the window is short enough that the Library folds into a row of circles.
  public var isShort: Bool

  /// Creates a fit; by default everything whole, as in a large window.
  ///
  /// - Parameters:
  ///   - sidebar: How the width leaves the sidebar; whole by default.
  ///   - footer: How the footer stands; whole by default.
  ///   - showsWhat: Whether the header keeps what its holder does; yes by default.
  ///   - isShort: Whether the Library folds into a row; no by default.
  public init(
    sidebar: ViewerSidebarMode = .full,
    footer: ViewerFooterFit = .full,
    showsWhat: Bool = true,
    isShort: Bool = false
  ) {
    self.sidebar = sidebar
    self.footer = footer
    self.showsWhat = showsWhat
    self.isShort = isShort
  }

  /// The fit for a window `size`, from this one: the sidebar by the width, then the footer and the header by the
  /// card's width beside the sidebar as it then stands, hidden if the person hid it, and the Library by the height.
  ///
  /// - Parameters:
  ///   - size: The window's size.
  ///   - isHidden: Whether the person hid the sidebar, which gives the card its room.
  /// - Returns: The fit.
  public func refitted(to size: CGSize, isHidden: Bool) -> ViewerFit {
    let sidebar = ViewerSidebarMode.fitting(windowWidth: size.width, current: sidebar)
    let card = ViewerLayout.cardWidth(windowWidth: size.width, mode: isHidden ? .hidden : sidebar)
    return ViewerFit(
      sidebar: sidebar,
      footer: ViewerFooterFit.fitting(cardWidth: card, current: footer),
      showsWhat: ViewerLayout.headerWhat.isAbove(card, wasAbove: showsWhat),
      isShort: !ViewerLayout.tallSidebar.isAbove(size.height, wasAbove: !isShort)
    )
  }
}
