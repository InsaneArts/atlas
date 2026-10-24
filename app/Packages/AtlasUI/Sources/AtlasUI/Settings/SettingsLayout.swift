import SwiftUI

/// The Settings window's own measures, which no other window shares, from the board (mac.py `settings_window`,
/// `form`, `board_settings`).
public enum SettingsLayout {
  /// 680 by 540: the window, which does not resize.
  public static let window = CGSize(width: 680, height: 540)
  /// 74: the tab row's height, the window buttons at its top and the tabs centred in it.
  static let tabRowHeight: CGFloat = 74
  /// 28: a pane's side inset.
  static let sideInset: CGFloat = 28
  /// 624: a pane's width inside its side insets.
  static let formWidth: CGFloat = window.width - 2 * sideInset
  /// 20 above and below, 28 at the sides: a pane's insets.
  static let paneInsets = EdgeInsets(top: Spacing.group, leading: sideInset, bottom: Spacing.group, trailing: sideInset)
  /// 18: between a pane's groups.
  static let groupGap: CGFloat = 18
  /// 380: the widest About's sentence runs before it wraps.
  static let aboutTextWidth: CGFloat = 380
  /// 96: the app's icon in About.
  static let aboutIcon: CGFloat = 96
}
