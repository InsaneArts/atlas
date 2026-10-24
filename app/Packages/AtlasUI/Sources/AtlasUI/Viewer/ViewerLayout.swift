import SwiftUI

/// The main window's own measures, which no other window shares, from the boards (mac.py `toolbar`, `window`).
enum ViewerLayout {
  /// 52: the top row's height, the title bar's with its empty toolbar.
  static let topRowHeight: CGFloat = 52
  /// 300: the Details pane's width.
  static let detailsWidth: CGFloat = 300
  /// 20: from the window's leading edge to the window buttons, and to the status in full screen, which hides them.
  static let lightsInset: CGFloat = 20
  /// 52: the three window buttons, 12 across and 8 apart.
  static let lightsWidth: CGFloat = 52
  /// 18: from the window buttons to the status.
  static let lightsToStatus: CGFloat = 18
  /// 16: between the top row's three columns.
  static let columnGap: CGFloat = 16
  /// 80: the room the status line asks for in the widest layout before the row gives it up for a narrower one; shown,
  /// the line takes whatever its column leaves and is cut short first (v3 V1).
  static let statusLineRoom: CGFloat = 80
  /// 60: the least room the status line shows in; with less, a few letters and an ellipsis would say nothing, so it goes.
  static let statusLineLeast: CGFloat = 60
  /// 116: the white capsule's least width, so Take Control and Give Back morph in place without the row moving (v3
  /// X5, `min-width: 116px`).
  static let mainButtonMinWidth: CGFloat = 116
  /// 3: between the control chip's keycaps (v3 `.kcs`).
  static let keycapGap: CGFloat = 3
  /// 1.5 by 13: the typing chip's caret (v3 `.typing .caret`).
  static let caret = CGSize(width: 1.5, height: 13)
  /// 14: the control rim's corner, the picture's 12 and the 2-point gap round it (v3 V5, `.vpic .rim`).
  static let rimRadius: CGFloat = 14
  /// 2: between the picture's edge and the rim, the ground showing through.
  static let rimGap: CGFloat = 2
  /// 2: the rim's accent line.
  static let rimWidth: CGFloat = 2
  /// 4: the soft accent ring outside the rim.
  static let ringWidth: CGFloat = 4
  /// 1.01: the rim settles in from a little larger.
  static let rimArrival: CGFloat = 1.01
  /// The top row's layouts, widest first, which a narrowing window steps through (v3 V15): the status line goes
  /// first, then the tabs' words, then the actions' words.
  static let barLayouts: [ViewerTopRowFit] = [.full, .statusWord, .tabSymbols, .actionSymbols]
  /// 26: the mini screen's inset from the window's right and bottom edges (mac.py `mini`).
  static let miniInset: CGFloat = 26
  /// 16 by 10: a screenshot's tile, and the screen's shape where a sample needs one.
  static let placeholderAspect = CGSize(width: 16, height: 10)
  /// 3: the most lines a notice over the screen takes, so a longer one is read whole yet never covers the picture.
  static let noticeLines = 3
  /// 520: the widest a notice over the screen runs before it wraps.
  static let noticeWidth: CGFloat = 520
  /// 18: a pane's text inset, the board's (mac.py `head`), which its header and rows keep, between the grid's 16 and
  /// 20.
  static let paneTextInset: CGFloat = 18
  /// 52: an app's row in the Apps pane, its 34 icon with 9 above and below.
  static let appRowHeight: CGFloat = 52
  /// 36: a window's row under its app.
  static let windowRowHeight: CGFloat = 36
  /// 34: the "No windows" row, a little lower than a window's.
  static let noWindowsHeight: CGFloat = 34
  /// 62: how far a window's row sits in, under its app's name (the board's 14 inset, 34 icon and 12 gap, and 2 more).
  static let windowIndent: CGFloat = 62
  /// 56 by 36: a screenshot's thumbnail on an Activity line, the board's 56 by 35 (the screen's 16 by 10) rounded to
  /// an even point.
  static let lineThumbnail = CGSize(width: 56, height: 36)
  /// 4: the Screenshots pane's columns.
  static let shotColumns = 4
  /// 18: between the Screenshots pane's rows of tiles, the board's, a little more than between its columns.
  static let shotRowGap: CGFloat = 18
  /// 40: how far a window outline's label sits above the outline's top.
  static let outlineLabelLift: CGFloat = 40
  /// 1040 by 650: the largest a screenshot is drawn in the large view (mac.py `board_shot_large`).
  static let largeShotLargest = CGSize(width: 1040, height: 650)

  /// How large the control rim is as control comes or goes: settled while in control, a little larger otherwise, so
  /// it settles in; never larger under Reduce Motion, where only its opacity changes.
  ///
  /// - Parameters:
  ///   - isInControl: Whether this window holds control.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The rim's scale.
  static func rimScale(isInControl: Bool, reduceMotion: Bool) -> CGFloat {
    guard !isInControl, !reduceMotion else { return 1 }
    return rimArrival
  }

  /// The status's leading inset: clear of the window buttons, or the buttons' own inset in full screen.
  ///
  /// - Parameter isFullScreen: Whether the window is in full screen, which hides its buttons.
  /// - Returns: The inset.
  static func statusInset(isFullScreen: Bool) -> CGFloat {
    isFullScreen ? lightsInset : lightsInset + lightsWidth + lightsToStatus
  }

  /// Where a window outline lies in a view showing the picture letterboxed at its own shape, as a click there maps
  /// (`DesktopPoint+Mapping`).
  ///
  /// - Parameters:
  ///   - outline: The window's edges as shares of the helper's display.
  ///   - view: The view's size.
  ///   - picture: The picture's size, in pixels.
  /// - Returns: The outline's frame in the view; empty when any size is.
  static func outlineFrame(_ outline: CGRect, in view: CGSize, picture: CGSize) -> CGRect {
    guard view.width > 0, view.height > 0, picture.width > 0, picture.height > 0 else { return .zero }
    let scale = min(view.width / picture.width, view.height / picture.height)
    let shown = CGSize(width: picture.width * scale, height: picture.height * scale)
    return CGRect(
      x: (view.width - shown.width) / 2 + outline.minX * shown.width,
      y: (view.height - shown.height) / 2 + outline.minY * shown.height,
      width: outline.width * shown.width,
      height: outline.height * shown.height
    )
  }

  /// The width a large screenshot leaves beside it: Previous and Next, each with its gap and the page margin.
  private static let largeShotBeside = (ControlHeight.mPlus + Spacing.mPlus + Spacing.xxxl) * 2
  /// The height a large screenshot leaves under it: the bar's row with its padding above and below, the gap over the
  /// bar, and the page margin top and bottom.
  private static let largeShotUnder = ControlHeight.sPlus + Spacing.xsPlus * 2 + Spacing.mPlus + Spacing.xxxl * 2

  /// How large a screenshot is drawn in the large view: at its own shape, within ``largeShotLargest``, and within a
  /// window `window` large with room for Previous and Next beside it and the bar under it.
  ///
  /// - Parameters:
  ///   - aspect: The screenshot's shape.
  ///   - window: The window's size.
  /// - Returns: The picture's size; zero when nothing fits.
  static func largeShotSize(aspect: CGSize, in window: CGSize) -> CGSize {
    let room = CGSize(
      width: min(largeShotLargest.width, window.width - largeShotBeside),
      height: min(largeShotLargest.height, window.height - largeShotUnder)
    )
    guard aspect.width > 0, aspect.height > 0, room.width > 0, room.height > 0 else { return .zero }
    let scale = min(room.width / aspect.width, room.height / aspect.height)
    return CGSize(width: aspect.width * scale, height: aspect.height * scale)
  }

}
