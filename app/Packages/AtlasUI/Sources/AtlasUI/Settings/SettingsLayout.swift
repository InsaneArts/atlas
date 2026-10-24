import SwiftUI

/// The Settings window's own measures, which no other window shares, from v3's Settings window (`.swin`, `.stabs`,
/// `.spane`, `.about`).
public enum SettingsLayout {
  /// 680: the window's width. Its height is its pane's, under the tab row (v3 G2).
  public static let width: CGFloat = 680
  /// 102: the title and the tab row over every pane (v3 `.sbody`'s top).
  public static let header: CGFloat = 102
  /// 540: the window's height before its first pane has said how tall it is.
  public static let firstHeight: CGFloat = 540
  /// 14: the title's top, in the title bar beside the window buttons (v3 `.stitle`).
  static let titleTop: CGFloat = 14
  /// 40: the tab row's top (v3 `.stabs`).
  static let tabsTop: CGFloat = 40
  /// 28: a pane's side inset.
  static let sideInset: CGFloat = 28
  /// 624: a pane's width inside its side insets.
  static let formWidth: CGFloat = width - 2 * sideInset
  /// 6 above, 28 at the sides and below: a form pane's insets (v3 `.spane`).
  static let paneInsets = EdgeInsets(top: Spacing.xsPlus, leading: sideInset, bottom: sideInset, trailing: sideInset)
  /// 20: between a form's sections, the gap and the next section name's own space above it (v3 `.spane`, `.sghead`).
  static let sectionGap = Spacing.xl
  /// 4: the space above a pane's first section name, when the pane opens with one (v3 Agents' "Connected").
  static let firstTitleInset = Spacing.xs
  /// 32 above, 68 at the sides, 62 below: About's insets, v3's `.about` padding inside its pane's (`.spane`).
  static let aboutInsets = EdgeInsets(top: 32, leading: 68, bottom: 62, trailing: 68)
  /// 84: the app's icon in About (v3 G9).
  static let aboutTile: CGFloat = 84
  /// 380: the widest About's sentence runs before it wraps.
  static let aboutTextWidth: CGFloat = 380
  /// 440: the widest About's update status runs before it wraps (v3 `.stl`).
  static let aboutStatusWidth: CGFloat = 440
  /// 18: About's update status's least height, so nothing under it moves as it comes and goes (v3 `.ust`).
  static let aboutStatusHeight: CGFloat = 18
  /// 220: About's install progress bar (v3 `.prog`).
  static let progressWidth: CGFloat = 220
  /// 40: the helper's picture at the top of the Helper pane (v3 `.shero`).
  static let accountPicture: CGFloat = 40
  /// 0.06: how long after the row above it each readiness check's mark pops in as Check Again's reading lands (v3
  /// G5).
  static let checkStaggerStep: TimeInterval = 0.06

  /// How long a readiness check's mark waits to pop in after Check Again's reading lands, so the rows pass one after
  /// another, top first (v3 G5).
  ///
  /// - Parameter index: The row's place, the first 0.
  /// - Returns: 60 milliseconds for each row above it.
  public static func checkStagger(index: Int) -> TimeInterval {
    TimeInterval(max(index, 0)) * checkStaggerStep
  }
}
