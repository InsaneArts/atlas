import SwiftUI

/// The menu bar panel's own measures, which no other window shares (mac.py `panel_card`).
enum PanelLayout {
  /// 352: the panel's width.
  static let width: CGFloat = 352
  /// 12: the panel's inset on every side.
  static let inset = Spacing.m
  /// 10: between the header, the line, the picture, Recent and the buttons.
  static let gap = Spacing.sPlus
  /// 328 by 205: the picture's frame, the width inside the inset at the screen's 16:10.
  static let picture = CGSize(width: 328, height: 205)
  /// 12: the orbit beside "Atlas", by its height; only the panel's header draws one this size.
  static let orbit: CGFloat = 12
  /// 22: the orbit on the empty picture, by its height, smaller than an empty message's 26 since no headline
  /// follows it.
  static let emptyOrbit: CGFloat = 22
  /// 30: the empty picture's side inset, so its line wraps well inside the frame.
  static let emptyInset: CGFloat = 30
  /// 26: a Recent row's least height.
  static let recentRow: CGFloat = 26
  /// 8: the caption's inset from the picture's corner, closer than the main window's 14 in the smaller frame.
  static let captionInset = Spacing.s
  /// The panel's ground: rgb(30, 30, 33) at 0.97 (mac.py:366).
  static let ground = Color(red: 30 / 255, green: 30 / 255, blue: 33 / 255)
    .opacity(0.97)
  /// 16: the panel's corners, where no popover holds it.
  static let corner: CGFloat = 16
  /// The panel's shadow, 0 20 50 black at 0.5 (mac.py:366): its colour.
  static let shadowColor = Color.black
    .opacity(0.5)
  /// 24: the shadow's radius, half the CSS 50 blur rounded to an even point, as ``Elevation`` halves them.
  static let shadowRadius: CGFloat = 24
  /// 20: how far the shadow falls below the panel.
  static let shadowDrop: CGFloat = 20
}
