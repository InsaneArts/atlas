import SwiftUI

/// The title sizes tried in order to fit the available width.
enum HeadlineLayout {
  /// 2: the most lines a statement takes at 24 points.
  static let titleLines = 2
  /// 3: the most lines each of statement and tail takes at 20 points, cut in the middle past them.
  static let steppedLines = 3
  /// 0.5: the part of a line the budget spares.
  static let spareLine: CGFloat = 0.5
  /// 29: one line at 24 points (``TextStyle/title``'s line height).
  static let titleLineHeight = TextStyle.title.lineHeight ?? TextStyle.title.pointSize
}
