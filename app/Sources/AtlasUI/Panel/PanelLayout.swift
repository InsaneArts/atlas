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
  /// [R] White at 0.14: the picture's edge, kept exact until Phase 3b uses `strokeStrong` (0.16).
  static let pictureEdge = Color.white
    .opacity(0.14)
}
