import SwiftUI

/// The Settings window's own measures, which no other window shares.
public enum SettingsLayout {
  /// 960: the first opening's width.
  public static let defaultWidth: CGFloat = 960
  /// 700: the first opening's height.
  public static let defaultHeight: CGFloat = 700
  /// 960: a tall viewport for overflow checks.
  public static let tallPreviewHeight: CGFloat = 960
  /// 880: the minimum width on a screen large enough to hold it.
  public static let minimumWidth: CGFloat = 880
  /// 560: the minimum height on a screen large enough to hold it.
  public static let minimumHeight: CGFloat = 560
  /// The default width used by standalone Settings previews.
  public static let width = defaultWidth
  /// The stable default height used by standalone Settings previews.
  public static let firstHeight = defaultHeight
  /// 224: the fixed sidebar width.
  static let sidebarWidth: CGFloat = 224
  /// 12: the sidebar's side and bottom insets.
  static let sidebarInsets = Spacing.m
  /// 52: clearance above the sidebar for native window buttons.
  static let sidebarTopInset: CGFloat = 52
  /// 2: between adjacent sidebar rows.
  static let sidebarRowGap = Spacing.half
  /// 20: between sidebar clusters.
  static let sidebarClusterGap = Spacing.xl
  /// 36: the minimum ordinary sidebar row height.
  static let sidebarRowHeight: CGFloat = 36
  /// 52: the Helper row's stable two-line slot.
  static let helperSidebarRowHeight: CGFloat = 52
  /// 64: the stationary navigation header height.
  static let headerHeight: CGFloat = 64
  /// 14: the header controls' distance from the full-size content top.
  static let headerTopInset = Spacing.mPlus
  /// 12: between history and the title.
  static let headerControlGap = Spacing.m
  /// 28: the page's side inset.
  static let pageSideInset: CGFloat = 28
  /// 28: below the page's content.
  static let pageBottomInset: CGFloat = 28
  /// 20: between the fixed header and page content.
  static let pageTopInset = Spacing.xl
  /// 624: the maximum readable content column width.
  static let contentMaximumWidth: CGFloat = 624
  /// The readable form width used by component samples.
  static let formWidth = contentMaximumWidth
  /// 28: standalone pane previews' horizontal inset.
  static let sideInset = pageSideInset
  /// The existing form content's insets inside the new page viewport.
  static let swapInsets = EdgeInsets(
    top: pageTopInset,
    leading: pageSideInset,
    bottom: pageBottomInset,
    trailing: pageSideInset
  )
  /// 3: the focus halo outside a sidebar selection.
  static let focusHalo: CGFloat = 3
  /// 20: between a form's sections, the gap and the next section name's own space above it.
  static let sectionGap = Spacing.xl
  /// 32 above, 68 at the sides, 62 below: About's insets.
  static let aboutInsets = EdgeInsets(top: 32, leading: 68, bottom: 62, trailing: 68)
  /// 84: the app's icon in About.
  static let aboutTile: CGFloat = 84
  /// 440: the widest About's sentence runs before it wraps, so it reads on one line.
  static let aboutTextWidth: CGFloat = 440
  /// 440: the widest About's update status runs before it wraps.
  static let aboutStatusWidth: CGFloat = 440
  /// 64: About's update status's height, its 14 above included, kept whatever it says, so the window never grows or
  /// shrinks as a check goes: room for two lines, such as the progress bar over its words or a problem over Show in
  /// Finder, the tallest it gets.
  static let aboutStatusHeight: CGFloat = 64
  /// 220: About's install progress bar.
  static let progressWidth: CGFloat = 220
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
