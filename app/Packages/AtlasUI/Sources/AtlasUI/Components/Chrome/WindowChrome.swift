import CoreGraphics

/// The measures every window with a sidebar and a content card shares, the main window's and Settings': where AppKit
/// draws the window buttons in a title bar an empty unified toolbar makes 52 high, and the card's inset and corner
/// beside the sidebar.
public enum WindowChrome {
  /// 20: from the window's leading edge to the window buttons.
  public static let lightsInset: CGFloat = 20
  /// 52: the three window buttons, 12 across and 8 apart.
  public static let lightsWidth: CGFloat = 52
  /// 52: the title bar's band an empty unified toolbar makes, the window buttons centred in it.
  public static let titleBand: CGFloat = 52
  /// 38: the bar across the content card's top, its centre about level with the window buttons': the main window's
  /// header, Settings' page title.
  public static let cardBarHeight: CGFloat = 38
  /// 8: the content card's inset from the window's top, trailing and bottom edges.
  public static let cardInset = Spacing.s
  /// 8: the content card's corner, the window's less its inset, so the two curves run side by side.
  public static let cardRadius = Radius.nested(in: Radius.window, inset: cardInset)
}
