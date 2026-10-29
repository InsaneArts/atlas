import SwiftUI

/// The Activity pane's measures: its top, its moments and their timeline, its steps' discs, keycaps, callouts and
/// thumbnails. Every moment's icon, line and disc sit in one column, so the eye runs down it.
extension ViewerLayout {
  /// 14: above the filters at the top of the list, the inset Apps keeps above its summary.
  static let activityTop = appsSummaryTop

  /// 4: above every moment's header, so folded moments sit 4 apart and the header never moves as it opens.
  static let momentAbove = Spacing.xs
  /// 8: under an open moment's last step.
  static let momentBelow = Spacing.s
  /// 40: a moment's header, open or folded, a true capsule when folded.
  static let momentHeight = ControlHeight.l
  /// 10: how far a header's and a step's capsule reach beyond the column, so icons, discs and words keep one column
  /// whether a capsule shows or not.
  static let capsuleReach = Spacing.sPlus
  /// 10: between a moment's icon and its title.
  static let momentIconGap = Spacing.sPlus
  /// 14: a session's icon inline in a moment's header.
  static let momentSessionIcon = IconSize.xsPlus
  /// 36: from the column's leading edge to a step's words, which line up with the moment's title: the icon's 26
  /// and its gap of 10.
  static let stepLeading = appIcon + momentIconGap
  /// 30: a step's least height.
  static let stepHeight = ControlHeight.sPlus
  /// 22: a step's disc.
  static let stepDisc: CGFloat = 22
  /// 1.5: the moment's line.
  static let timeline = Stroke.emphasis
  /// 6: the gap above and below a step's thumbnail, and around its callout.
  static let stepInset = Spacing.xsPlus
  /// 132: a step's thumbnail's width.
  static let stepThumbnail: CGFloat = 132
  /// 8: a step's thumbnail's corner.
  static let stepThumbnailRadius = Radius.small
  /// 4:3 to 16:9: the narrowest and widest a step's thumbnail stands.
  static let shotAspects: ClosedRange<CGFloat> = (4.0 / 3.0)...(16.0 / 9.0)
  /// 14: a callout's corner, a 28-point capsule's, so a one-line callout is a true capsule.
  static let calloutRadius: CGFloat = 14
  /// 12: a callout's side inset.
  static let calloutInset = Spacing.m
  /// 2: between a callout's lines, and between a key's caps.
  static let calloutGap = Spacing.half
  /// 6: between two chords of keys.
  static let chordGap = Spacing.xsPlus
  /// 24: a quiet day heading's height above its moments.
  static let dayHeight = ControlHeight.xs
  /// 12: above a day heading.
  static let dayAbove = Spacing.m
}
