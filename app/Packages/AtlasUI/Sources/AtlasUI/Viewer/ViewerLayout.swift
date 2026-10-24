import SwiftUI

/// The main window's own measures, which no other window shares, from the boards (mac.py `toolbar`, `window`).
enum ViewerLayout {
  /// 52: the top row's height, the title bar's with its empty toolbar.
  static let topRowHeight: CGFloat = 52
  /// 280: Details' width, an inspector beside the stage (v3 V3, `.vdetails .in`).
  static let detailsWidth: CGFloat = 280
  /// 292: the room Details takes from the stage while it shows: its own width and the window's inset to its right
  /// (v3 `.vwin.details .vdetails`).
  static let detailsColumn = detailsWidth + Spacing.m
  /// 14: Details' inset round its content, and between its header and groups (v3 `.vdetails .in`).
  static let detailsInset = Spacing.mPlus
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
  /// 16: a pane's inset at its sides, which its header, rows and groups keep (v3 `.vlhead`, `.arow`, `.appg`).
  static let paneInset = Spacing.l
  /// 1: how long a new live line in Activity keeps its blue ring before it cools away (v3 V8, X10).
  static let liveHold = 1.0
  /// 46 by 29: a screenshot's thumbnail on an Activity line (v3 `.thumbm`).
  static let lineThumbnail = CGSize(width: 46, height: 29)
  /// 12: above the first of the Apps pane's groups, which sit 10 apart (v3 `.vlbody`'s `padding-top`, `.appg`).
  static let appsTop = Spacing.m
  /// 26: an app's icon at the head of its group (v3 `.appi`).
  static let appIcon = IconSize.mPlus
  /// 34: a window's row under its app, and the "No windows" row (v3 V9, `.wrow`).
  static let windowRowHeight = ControlHeight.mPlus
  /// 48: how far a window's row and its hairline sit in, under its app's name: the group's 12, the icon's 26 and 10
  /// (v3 `.wrow`).
  static let windowIndent: CGFloat = 48
  /// 280: the least width a screenshot's tile takes before the grid drops a column, so a full window holds four and
  /// a narrow one two (v3 V10's adaptive grid).
  static let shotLeastWidth: CGFloat = 280
  /// 14: between the Screenshots pane's tiles, across and down (v3 `.sgrid`).
  static let shotGap = Spacing.mPlus
  /// 2: how far a screenshot's tile lifts under the pointer (v3 V10, `.stile:hover`).
  static let shotLift: CGFloat = 2
  /// 1.015: how much it grows as it lifts.
  static let shotLiftScale: CGFloat = 1.015
  /// 0.72: the shade over the window behind a screenshot shown large (v3 V10, `.vlarge .scrim`).
  static let shotScrim = 0.72
  /// 6: how much the window blurs behind it.
  static let shotScrimBlur: CGFloat = 6
  /// 0.22: how long the shade and the blur take to fade in or out.
  static let shotScrimFade = 0.22
  /// The shade's, the blur's and the large view's buttons' fade, in and out: ease out over ``shotScrimFade``.
  static var shotFade: Animation {
    .easeOut(duration: shotScrimFade)
  }
  /// 26: from the window's bottom to the large screenshot's bar (v3 `.vlarge .lbar`).
  static let shotBarInset: CGFloat = 26
  /// 16: how far a new pane comes in from, on the side it comes from (v3 X3).
  static let paneEnter: CGFloat = 16
  /// 12: how far the old pane goes, toward the other side, as it fades.
  static let paneLeave: CGFloat = 12
  /// 0.05: how long the old pane shows alone before the new one starts in, so the stage is never blank (v3 X3, R15).
  static let paneOverlap = 0.05
  /// 0.36: how dark the picture goes outside a window shown from the Apps tab (v3 V14, `.outline`).
  static let spotlightDim = 0.36
  /// 14: how much the last picture blurs behind Connecting's words (v3 V7, X4, `.vempty .lastframe`).
  static let lastFrameBlur: CGFloat = 14
  /// 0.6: how much colour the last picture keeps behind them.
  static let lastFrameSaturation = 0.6
  /// 0.55: the black laid over it, so it keeps 0.45 of its brightness, as CSS `brightness(.45)` multiplies.
  static let lastFrameShade = 0.55
  /// 1.04: how much larger the blurred picture draws than the stage, so its blurred edges fall outside it.
  static let lastFrameScale: CGFloat = 1.04
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

  /// The room the stage leaves at its trailing edge: Details' column while Details shows, else none. It follows
  /// whether Details shows, never how far it has slid, so the picture takes its new width at once and never squeezes
  /// while Details moves (v3 V3).
  ///
  /// - Parameter showsDetails: Whether Details shows.
  /// - Returns: The trailing room.
  static func stageTrailing(showsDetails: Bool) -> CGFloat {
    showsDetails ? detailsColumn : .zero
  }

  /// How far Details sits from its place: there while it shows, else its whole column further out, past the
  /// window's edge. Under Reduce Motion it stays in place and fades instead.
  ///
  /// - Parameters:
  ///   - isShown: Whether Details shows.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: Its horizontal offset.
  static func detailsOffset(isShown: Bool, reduceMotion: Bool) -> CGFloat {
    guard !isShown, !reduceMotion else { return .zero }
    return detailsColumn
  }

  /// How opaque Details draws: whole, except hidden under Reduce Motion, where it fades out in place instead of
  /// sliding away.
  ///
  /// - Parameters:
  ///   - isShown: Whether Details shows.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: Its opacity.
  static func detailsOpacity(isShown: Bool, reduceMotion: Bool) -> Double {
    guard !isShown, reduceMotion else { return 1 }
    return 0
  }

  /// How far a pane sits from its place as a tab switch brings it in or takes it away (v3 X3): the new pane comes in
  /// from ``paneEnter`` on the side the tab lies, the old one leaves ``paneLeave`` toward the other side; none under
  /// Reduce Motion, where the panes only crossfade.
  ///
  /// - Parameters:
  ///   - isArriving: Whether the pane comes in, rather than goes.
  ///   - direction: Which way the switch went, from ``ViewerTab/direction(from:to:)``: 1 to the right, −1 back.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: Its horizontal offset at its start, coming in, or at its end, going.
  static func paneOffset(isArriving: Bool, direction: Int, reduceMotion: Bool) -> CGFloat {
    guard !reduceMotion else { return .zero }
    let side = CGFloat(direction)
    return isArriving ? paneEnter * side : -paneLeave * side
  }

  /// How much the window blurs behind a screenshot shown large: ``shotScrimBlur`` while one shows, else none.
  ///
  /// - Parameter isShown: Whether a screenshot shows large.
  /// - Returns: The blur's radius.
  static func shotBlur(isShown: Bool) -> CGFloat {
    isShown ? shotScrimBlur : .zero
  }

  /// How far a screenshot's tile rises under the pointer: ``shotLift``, or not at all at rest or under Reduce Motion.
  ///
  /// - Parameters:
  ///   - isHovered: Whether the pointer is over it.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: Its upward lift.
  static func shotRise(isHovered: Bool, reduceMotion: Bool) -> CGFloat {
    guard isHovered, !reduceMotion else { return .zero }
    return shotLift
  }

  /// How large a screenshot's tile draws under the pointer: ``shotLiftScale``, or its own size at rest or under
  /// Reduce Motion.
  ///
  /// - Parameters:
  ///   - isHovered: Whether the pointer is over it.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: Its scale.
  static func shotScale(isHovered: Bool, reduceMotion: Bool) -> CGFloat {
    guard isHovered, !reduceMotion else { return 1 }
    return shotLiftScale
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
  /// The height a large screenshot leaves above and below it, as it is centred: the bar at the window's foot, its
  /// inset under it and a gap over it, on each side.
  private static let largeShotUnder = (shotBarInset + ControlHeight.mPlus + Spacing.mPlus) * 2

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
