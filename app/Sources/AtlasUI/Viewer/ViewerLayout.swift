import CoreGraphics

/// The viewer window's own measures, which no other window shares.
enum ViewerLayout {
  /// 1000: below this width the sidebar would squeeze the desktop, so it waits.
  static let sidebarRoom: CGFloat = 1000
  /// 300: the sidebar's width.
  static let sidebarWidth: CGFloat = 300
  /// 44: the header's least height, when the title bar reports none.
  static let headerHeight: CGFloat = 44
  /// 62: the footer's height.
  static let footerHeight: CGFloat = 62
  /// 22: a bar's side inset: the footer's, and the header's in full screen.
  static let barInset: CGFloat = 22
  /// 80: the header's leading inset, clear of the window buttons.
  static let windowButtonsInset: CGFloat = 80
  /// 22: the orbit mark in the header.
  static let mark: CGFloat = 22
  /// 28: around the stage's placeholder.
  static let placeholderInset: CGFloat = 28
  /// 380: the recovery alert's note, narrower than its lede.
  static let recoveryNoteWidth: CGFloat = 380
  /// 3: the most lines a notice over the stage takes, so a longer one is read whole yet never covers the picture.
  static let noticeLines = 3
  /// 520: the widest a notice over the stage runs before it wraps.
  static let noticeWidth: CGFloat = 520
  /// 16 by 10: the stage's shape before the first picture arrives.
  static let placeholderAspect = CGSize(width: 16, height: 10)
  /// 0.45: a paused picture, dimmed so it reads as not live.
  static let pausedPicture = 0.45
  /// 0.6: a caption older than the newest.
  static let olderCaption = 0.6
  /// 0.9: the accent on the stage's edge while in control.
  static let controlEdge = 0.9
  /// 0.3: the accent glowing around the stage while in control.
  static let controlGlow = 0.3
  /// 14: the control glow's blur radius.
  static let controlGlowRadius: CGFloat = 14

  /// Whether the sidebar shows: the person wants it and the window is at least ``sidebarRoom`` wide.
  ///
  /// - Parameters:
  ///   - width: The window's width.
  ///   - wanted: Whether the person asked for the sidebar.
  /// - Returns: Whether it shows.
  static func showsSidebar(width: CGFloat, wanted: Bool) -> Bool {
    wanted && width >= sidebarRoom
  }

  /// The stage as drawn beside the sidebar or not: the sidebar lists the recent actions, so the captions over the
  /// desktop step aside while it shows, and only then; a sidebar wanted in a window too narrow for it hides nothing.
  ///
  /// - Parameters:
  ///   - stage: The stage as the presenter built it.
  ///   - besideSidebar: Whether the sidebar shows now (``showsSidebar(width:wanted:)``).
  /// - Returns: The stage to draw.
  static func stage(_ stage: ViewerStageState, besideSidebar: Bool) -> ViewerStageState {
    guard besideSidebar else { return stage }
    var shown = stage
    shown.captions = []
    return shown
  }
}
