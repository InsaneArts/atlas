import SwiftUI

/// The chip's own measures for its floating shadow.
enum PictureChipLayout {
  /// 11.5: the small chip's words, between the scale's caption and callout.
  static let smallText: CGFloat = 11.5
  /// 7: between the small chip's parts.
  static let smallGap: CGFloat = 7
  /// 7: the shadow's blur, half the equivalent 14-point CSS blur.
  static let shadowRadius: CGFloat = 7
  /// 4: how far the shadow falls.
  static let shadowDrop: CGFloat = 4
  /// 24: how far past the chip its shadow may spread, more than its blur and drop together.
  static let shadowRoom = Spacing.xxl
}
