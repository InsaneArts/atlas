import SwiftUI

/// The menu bar panel's own measures, which no other window shares.
enum PanelLayout {
  /// 320: the popover's width.
  static let width: CGFloat = 320
  /// 16:10: the picture's frame; the picture letterboxes into it.
  static let pictureAspect: CGFloat = 16 / 10
  /// −6: the menu rows reach past the content's inset, so their hover lines up with the cards' edges as a
  /// menu's does.
  static let menuOutset: CGFloat = -6
  /// 24: the orbit mark beside the title.
  static let mark: CGFloat = 24
  /// The picture's edge.
  static let pictureEdge = Color.strokeStrong
}
