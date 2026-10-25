import SwiftUI

/// The Settings window's own measures, which no other window shares.
public enum SettingsLayout {
  /// 680: the window's width. Its height is its pane's, under the tab row.
  public static let width: CGFloat = 680
  /// 102: the title and the tab row over every pane.
  public static let header: CGFloat = 102
  /// 540: the window's height before its first pane has said how tall it is.
  public static let firstHeight: CGFloat = 540
  /// 14: the title's top, in the title bar beside the window buttons.
  static let titleTop: CGFloat = 14
  /// 40: the tab row's top.
  static let tabsTop: CGFloat = 40
  /// 28: a pane's side inset.
  static let sideInset: CGFloat = 28
  /// 624: a pane's width inside its side insets.
  static let formWidth: CGFloat = width - 2 * sideInset
  /// 6 above, 28 at the sides and below: a form pane's insets.
  static let swapInsets = EdgeInsets(top: Spacing.xsPlus, leading: sideInset, bottom: sideInset, trailing: sideInset)
  /// 20: between a form's sections, the gap and the next section name's own space above it.
  static let sectionGap = Spacing.xl
  /// About uses the same column insets as every other Settings pane.
  static let aboutInsets = swapInsets
  /// 68: the app's visible icon tile in About's compact header.
  static let aboutTile: CGFloat = 68
  /// 360: the widest About's tagline runs, also constrained by the available column.
  static let aboutTextWidth: CGFloat = 360
  /// 40: the helper's picture at the top of the Helper pane.
  static let accountPicture: CGFloat = 40
  /// 10: between a section's name and its group, as ``GroupedSection`` sets them, for a section that draws its own
  /// name.
  static let titleGap = Spacing.sPlus
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
