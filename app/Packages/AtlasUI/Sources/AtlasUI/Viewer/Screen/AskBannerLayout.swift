import SwiftUI

/// The measures of the question over the viewer's Screen, the panel card's family: its corner, its insets and how far
/// it keeps from the Screen's edges.
enum AskBannerLayout {
  /// 720: the widest it draws, so a wide window keeps its answers near its words while a question such as "Click
  /// “Merge pull request” in github.com?" reads whole beside its three answers.
  static let width: CGFloat = 720
  /// 10: how far it keeps from the Screen's bottom and sides.
  static let edge = Spacing.sPlus
  /// 14: its corner, the panel card's.
  static let corner = AskCardLayout.corner
  /// Its inset round its content: 8, and 12 before the agent's mark, so the mark sits as far in as the corner reads.
  static let inset = EdgeInsets(top: Spacing.s, leading: Spacing.m, bottom: Spacing.s, trailing: Spacing.s)
  /// 8: between its mark, its words and its answers.
  static let gap = Spacing.s
  /// 1: between the question and the line under it.
  static let lineGap = Spacing.hairline
  /// 160: the least the words keep beside the answers in one row, the question cut short past it; narrower, the
  /// answers go under the words.
  static let leadInRow: CGFloat = 160
}
