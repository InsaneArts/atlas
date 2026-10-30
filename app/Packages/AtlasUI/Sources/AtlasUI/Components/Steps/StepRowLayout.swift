import SwiftUI

/// The step row's own measures.
enum StepRowLayout {
  /// 48: a setup list's row.
  static let height: CGFloat = 48
  /// 2: the most lines its title or detail takes, a long name cut in its middle.
  static let lines = 2
  /// 50: where the separator above a step starts, past the disc.
  static let separatorInset: CGFloat = 50
  /// 24.25: the rail's leading edge, so its 1.5 points centre on the disc: the row's 14 inset plus half the disc,
  /// less half the rail.
  static let railInset = Spacing.mPlus + StepDisc.side / 2 - Stroke.emphasis / 2
}
