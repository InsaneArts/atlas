import SwiftUI

/// The status line's own measures for its mark and text.
enum StatusLineLayout {
  /// 7: between the mark and the sentence.
  static let gap: CGFloat = 7
  /// 1.5: the mark's inset from the top, so a 13-point mark centres on the first 16-point line.
  static let markInset: CGFloat = 1.5
  /// 0.5: the same on a 14-point caption line.
  static let smallMarkInset: CGFloat = 0.5
}
