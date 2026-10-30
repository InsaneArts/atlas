import AppKit
import AtlasUI

/// The badge's own measures.
enum StatusBadgeLayout {
  /// 0.5: the badge's opacity at the bottom of a breath.
  static let breathLow = 0.5
  /// 1: above anything the button draws in its own layer.
  static let above: CGFloat = 1
  /// 0: the badge's scale as a pop starts, nothing.
  static let nothing: CGFloat = 0
  /// 1: the badge whole, the scale a pop lands on and the opacity the breath settles back to.
  static let whole: CGFloat = 1
}
