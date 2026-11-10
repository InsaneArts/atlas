import SwiftUI

/// The Settings window's own measures, which no other window shares, set as Rex's Settings stands: a 740 by 572
/// window, a 214 sidebar of 32-point page rows under a 40-point status row, and the page on the content card under a
/// 52 bar of back and forward and the title.
public enum SettingsLayout {
  /// 740: the window's width.
  public static let width: CGFloat = 740
  /// 572: the window's height.
  public static let height: CGFloat = 572
  /// 214: the sidebar's width.
  static let sidebarWidth: CGFloat = 214
  /// 518: the content card's width, beside the sidebar and inside the window's trailing inset.
  static let cardWidth = width - sidebarWidth - WindowChrome.cardInset
  /// 478: a page's sections across, inside the page's side insets.
  static let pageWidth = cardWidth - pageInsets.leading - pageInsets.trailing
  /// 10: the sidebar's inset at its sides, which its rows' capsules keep to.
  static let sidebarInset = Spacing.sPlus
  /// 12: under the search, above the Helper row.
  static let searchGap = Spacing.m
  /// 40: the Helper row, its 28-point mark and the line under its title.
  static let helperRowHeight = ControlHeight.l
  /// 32: a page's row.
  static let pageRowHeight = ControlHeight.m
  /// 6: a row's content inset inside its capsule.
  static let pageRowInset = Spacing.xsPlus
  /// 8: between a row's tile and its title.
  static let pageRowGap = Spacing.s
  /// 13: between two groups of rows, as Rex's sidebar leaves.
  static let groupGap: CGFloat = 13
  /// 20: a page's coloured tile.
  static let pageTile: CGFloat = 20
  /// 5: the tile's corner, a quarter of its side.
  static let pageTileCorner = pageTile / 4
  /// 12: the tile's symbol.
  static let pageTileSymbol = IconSize.xs
  /// 38: the bar over the page inside the card, as the main window's header stands, so the back and forward capsule
  /// centres about level with the window buttons.
  static let barHeight = WindowChrome.cardBarHeight
  /// 10: the bar's inset before the capsule.
  static let barLeading = Spacing.sPlus
  /// 16: between the capsule and the title.
  static let barGap = Spacing.l
  /// 32: the back and forward capsule's height, clear of the card's edge inside the bar.
  static let navHeight = ControlHeight.m
  /// 34: each half of the capsule.
  static let navHalf = ControlHeight.mPlus
  /// 18: the hairline between the halves.
  static let navDivider: CGFloat = 18
  /// The page's insets on the card: 12 under the bar, 20 at the sides and 28 at the foot, so its last control ends
  /// clear of the fade.
  static let pageInsets = EdgeInsets(top: Spacing.m, leading: Spacing.xl, bottom: pageFoot, trailing: Spacing.xl)
  /// 28: under the page's last section.
  static let pageFoot: CGFloat = 28
  /// 12: how far the page fades out under the bar and at the card's foot.
  static let pageFade = Spacing.m
  /// 10: between a section's name and its group, for a section that draws its own name.
  static let titleGap = Spacing.sPlus
  /// 30: between a page's sections.
  static let sectionGap: CGFloat = 30
  /// 3: how far a page rises from as it comes in.
  static let pageRise: CGFloat = 3
  /// 12: how far an account's page slides in from the trailing edge, and back out.
  static let pushShift = Spacing.m
  /// 56: the Helper page's status card.
  static let headerRowHeight: CGFloat = 56
  /// 38: the symbol over an empty card's headline.
  static let emptySymbol: CGFloat = 38
  /// 330: the widest an empty card's sentence runs.
  static let emptyTextWidth: CGFloat = 330
  /// 64: the app's icon on About.
  static let aboutTile = IconSize.xxl
  /// 300: the widest About's tagline runs.
  static let aboutTextWidth: CGFloat = 300
  /// 72: a number the person types, such as how many days pictures are kept.
  static let numberWidth: CGFloat = 72
  /// 0.05: how long after the row above it each readiness check's result lands as Check Again's reading ends.
  static let checkStaggerStep: TimeInterval = 0.05

  /// How long a readiness check's result waits to land after Check Again's reading ends, so the rows land one after
  /// another, top first.
  ///
  /// - Parameter index: The row's place, the first 0.
  /// - Returns: 50 milliseconds for each row above it.
  public static func checkStagger(index: Int) -> TimeInterval {
    TimeInterval(max(index, 0)) * checkStaggerStep
  }
}
