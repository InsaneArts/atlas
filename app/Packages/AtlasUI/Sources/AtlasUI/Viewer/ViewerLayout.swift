import SwiftUI

/// The main window's own measures, which no other window shares.
enum ViewerLayout {
  /// 52: the top row's height, the title bar's with its empty toolbar.
  static let topRowHeight: CGFloat = 52
  /// 280: Details' width, an inspector beside the stage.
  static let detailsWidth: CGFloat = 280
  /// 292: the room Details takes from the stage while it shows: its own width and the window's inset to its
  /// right.
  static let detailsColumn = detailsWidth + Spacing.m
  /// 14: Details' inset round its content, and between its header and groups.
  static let detailsInset = Spacing.mPlus
  /// 20: from the window's leading edge to the window buttons, and to the status in full screen, which hides them.
  static let lightsInset: CGFloat = 20
  /// 52: the three window buttons, 12 across and 8 apart.
  static let lightsWidth: CGFloat = 52
  /// 18: from the window buttons to the status.
  static let lightsToStatus: CGFloat = 18
  /// 16: between the top row's three columns.
  static let columnGap: CGFloat = 16
  /// 80: the room the status line asks for in the widest layout before the row gives it up for a narrower
  /// one; shown, the line takes whatever its column leaves and is cut short first.
  static let statusLineRoom: CGFloat = 80
  /// 60: the least room the status line shows in; with less, a few letters and an ellipsis would say nothing, so it goes.
  static let statusLineLeast: CGFloat = 60
  /// 116: the white capsule's least width, so Take Control and Give Back morph in place without the row
  /// moving.
  static let mainButtonMinWidth: CGFloat = 116
  /// 3: between the control chip's keycaps.
  static let keycapGap: CGFloat = 3
  /// 1.5 by 13: the typing chip's caret.
  static let caret = CGSize(width: 1.5, height: 13)
  /// 14: the control rim's corner, the picture's 12 and the 2-point gap round it.
  static let rimRadius: CGFloat = 14
  /// 2: between the picture's edge and the rim, the ground showing through.
  static let rimGap: CGFloat = 2
  /// 2: the rim's accent line.
  static let rimWidth: CGFloat = 2
  /// 4: the soft accent ring outside the rim.
  static let ringWidth: CGFloat = 4
  /// 1.01: the rim settles in from a little larger.
  static let rimArrival: CGFloat = 1.01
  /// The top row's layouts, widest first, which a narrowing window steps through: the status line goes first, then
  /// the tabs' words, then the picture's pause, then the actions' words, so Stop and Resume never stand as symbols
  /// beside the pause and read as one player's buttons.
  static let barLayouts: [ViewerTopRowFit] = [.full, .statusWord, .tabSymbols, .noPause, .actionSymbols]
  /// 26: the mini screen's inset from the stage's edges.
  static let miniInset: CGFloat = 26
  /// 248: the widest the mini screen draws, in a wide window.
  static let miniWidest: CGFloat = 248
  /// 176: the narrowest it draws, in a narrow window.
  static let miniNarrowest: CGFloat = 176
  /// 0.22: the share of the stage's width it takes between the two.
  static let miniShare: CGFloat = 0.22
  /// 16 by 10: its shape, the screen's.
  static let miniAspect = CGSize(width: 16, height: 10)
  /// 248 by 155: the mini screen at its widest.
  static let miniLargest = miniSize(stageWidth: .infinity)
  /// 4: how far the pointer moves on the mini screen before a press becomes a drag; less is a click, back to the
  /// screen.
  static let miniDragStart: CGFloat = 4
  /// 0.96: how small the mini screen starts as it fades in without growing from the stage, as from Screenshots.
  static let miniArrival: CGFloat = 0.96
  /// The spring the mini screen lands on in the corner it was thrown toward: 0.4 seconds with a little bounce, the one
  /// bounce in the window, since the person threw it.
  static let miniThrown = Spring(duration: 0.4, bounce: 0.1)
  /// 1100: below this window width Details floats over the stage's trailing edge instead of taking its room, so a
  /// narrow window keeps a picture worth looking at.
  static let detailsFloatsBelow: CGFloat = 1100
  /// 16 by 10: a screenshot's tile, and the screen's shape where a sample needs one.
  static let placeholderAspect = CGSize(width: 16, height: 10)
  /// 3: the most lines a notice over the screen takes, so a longer one is read whole yet never covers the picture.
  static let noticeLines = 3
  /// 520: the widest a notice over the screen runs before it wraps.
  static let noticeWidth: CGFloat = 520
  /// 16: a pane's inset at its sides, which its header, rows and groups keep.
  static let swapInset = Spacing.l
  /// 1: how long a new live line in Activity keeps its blue ring before it cools away.
  static let liveHold = 1.0
  /// 46 by 29: a screenshot's thumbnail on an Activity line.
  static let lineThumbnail = CGSize(width: 46, height: 29)
  /// 12: above the first of the Apps pane's groups, which sit 10 apart.
  static let appsTop = Spacing.m
  /// 26: an app's icon at the head of its group.
  static let appIcon = IconSize.mPlus
  /// 34: a window's row under its app, and the "No windows" row.
  static let windowRowHeight = ControlHeight.mPlus
  /// 48: how far a window's row and its hairline sit in, under its app's name: the group's 12, the icon's 26
  /// and 10.
  static let windowIndent: CGFloat = 48
  /// 280: the least width a screenshot's tile takes before the grid drops a column, so a full window holds
  /// four and a narrow one two.
  static let shotLeastWidth: CGFloat = 280
  /// 14: between the Screenshots pane's tiles, across and down.
  static let shotGap = Spacing.mPlus
  /// 0.72: the shade over the window behind a screenshot shown large; the window behind is not blurred, the shade is
  /// enough.
  static let shotScrim = 0.72
  /// The shade's and the large view's bar's fade, in and out: ``Motion/fade``, which only changes opacity, so
  /// Reduce Motion keeps it.
  static var shotFade: Animation { Motion.fade }
  /// 26: from the window's bottom to the large screenshot's bar.
  static let shotBarInset: CGFloat = 26
  /// 0.36: how dark the picture goes outside a window shown from the Apps tab.
  static let spotlightDim = 0.36
  /// 14: how much the last picture blurs behind Connecting's words.
  static let lastFrameBlur: CGFloat = 14
  /// 0.6: how much colour the last picture keeps behind them.
  static let lastFrameSaturation = 0.6
  /// 0.55: the black laid over it, so it keeps 0.45 of its brightness, as CSS `brightness(.45)` multiplies.
  static let lastFrameShade = 0.55
  /// 1.04: how much larger the blurred picture draws than the stage, so its blurred edges fall outside it.
  static let lastFrameScale: CGFloat = 1.04
  /// 1040 by 650: the largest a screenshot is drawn in the large view, leaving room for its controls.
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

  /// Whether Details floats over the stage's trailing edge rather than taking its room: in a window narrower than
  /// ``detailsFloatsBelow``.
  ///
  /// - Parameter windowWidth: The window's width.
  /// - Returns: Whether Details floats.
  static func detailsFloats(windowWidth: CGFloat) -> Bool {
    windowWidth < detailsFloatsBelow
  }

  /// The room the stage leaves at its trailing edge: Details' column while Details shows and takes its room, else
  /// none. It moves on ``stageSlide(tab:reduceMotion:)``, Details' own spring, so the stage's edge keeps to the
  /// column's as it slides.
  ///
  /// - Parameters:
  ///   - showsDetails: Whether Details shows.
  ///   - floats: Whether Details floats over the stage instead.
  /// - Returns: The trailing room.
  static func stageTrailing(showsDetails: Bool, floats: Bool = false) -> CGFloat {
    showsDetails && !floats ? detailsColumn : .zero
  }

  /// How the stage gives Details its room or takes it back: on the Screen, on ``Motion/slide``, the spring Details
  /// slides on, so the picture shrinks and recentres in step with the column, turning back with it when Details is
  /// toggled again mid-way; the live picture only rescales on its layer meanwhile. Activity, Apps and Screenshots take
  /// their new width at once, in the same moment Details fades in or out over them: laying a list or a grid out again
  /// at every frame of a slide costs about 10 milliseconds a frame, and a grid's columns would jump between rows. At
  /// once under Reduce Motion too, where Details only fades.
  ///
  /// - Parameters:
  ///   - tab: The tab shown.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The animation, or nil.
  static func stageSlide(tab: ViewerTab, reduceMotion: Bool) -> Animation? {
    guard detailsSlides(tab: tab, reduceMotion: reduceMotion) else { return nil }
    return Motion.slide
  }

  /// Whether Details slides in from past the window's edge: on the Screen, where the picture shrinks beside it as it
  /// comes, so the column's arrival reads as one move; elsewhere it fades in place, in the same moment the pane takes
  /// its new width, so no blank strip shows while it travels. Never under Reduce Motion.
  ///
  /// - Parameters:
  ///   - tab: The tab shown.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: Whether it slides.
  static func detailsSlides(tab: ViewerTab, reduceMotion: Bool) -> Bool {
    tab == .screen && !reduceMotion
  }

  /// How Details comes and goes: sliding on ``Motion/slide``, the stage's spring, so the two keep to each other; else
  /// fading in place, in over ``Motion/Duration/swapIn`` and out over ``Motion/Duration/swapOut``, as a tab's pane does.
  ///
  /// - Parameters:
  ///   - slides: Whether it slides (``detailsSlides(tab:reduceMotion:)``).
  ///   - isShown: Whether it is coming in, rather than going.
  /// - Returns: The animation.
  static func detailsMotion(slides: Bool, isShown: Bool) -> Animation {
    if slides { return Motion.slide }
    return isShown ? Motion.swapIn : Motion.swapOut
  }

  /// How large the mini screen draws as it comes in: from ``miniArrival`` while it fades in on its own; its own size
  /// as it grows from the stage, which moves it instead, once in, and under Reduce Motion, where it only fades.
  ///
  /// - Parameters:
  ///   - isComing: Whether it is about to come in, at its transition's start.
  ///   - growsFromStage: Whether it arrives as the stage's picture moving into the corner.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: Its scale.
  static func miniScale(isComing: Bool, growsFromStage: Bool, reduceMotion: Bool) -> CGFloat {
    guard isComing, !growsFromStage, !reduceMotion else { return 1 }
    return miniArrival
  }

  /// The mini screen's size in a stage `stageWidth` wide: ``miniShare`` of it, kept between ``miniNarrowest`` and
  /// ``miniWidest``, at the screen's shape.
  ///
  /// - Parameter stageWidth: The stage's width.
  /// - Returns: Its size.
  static func miniSize(stageWidth: CGFloat) -> CGSize {
    let width = min(max(stageWidth * miniShare, miniNarrowest), miniWidest)
    return CGSize(width: width, height: width * miniAspect.height / miniAspect.width)
  }

  /// How far the panes' lists keep their content clear of the mini screen at their end nearest it: its height and its
  /// inset on both sides of it, so the last row can be scrolled out from under it.
  ///
  /// - Parameter stageWidth: The stage's width.
  /// - Returns: The margin.
  static func miniRoom(stageWidth: CGFloat) -> CGFloat {
    miniSize(stageWidth: stageWidth).height + miniInset * 2
  }

  /// How the mini screen lands in its corner once let go of: on ``miniThrown`` from wherever the drag left it, going
  /// on at the speed it was thrown; at once under Reduce Motion.
  ///
  /// - Parameters:
  ///   - velocity: Its speed toward the corner as it was let go, as a share of the way left each second.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The animation, or nil.
  static func miniLanding(velocity: Double, reduceMotion: Bool) -> Animation? {
    guard !reduceMotion else { return nil }
    return .interpolatingSpring(miniThrown, initialVelocity: velocity)
  }

  /// How far Details sits from its place: there while it shows, else its whole column further out, past the
  /// window's edge, when it slides; in place when it fades.
  ///
  /// - Parameters:
  ///   - isShown: Whether Details shows.
  ///   - slides: Whether it slides (``detailsSlides(tab:reduceMotion:)``).
  /// - Returns: Its horizontal offset.
  static func detailsOffset(isShown: Bool, slides: Bool) -> CGFloat {
    guard !isShown, slides else { return .zero }
    return detailsColumn
  }

  /// How opaque Details draws: whole, except hidden when it fades rather than slides away.
  ///
  /// - Parameters:
  ///   - isShown: Whether Details shows.
  ///   - slides: Whether it slides (``detailsSlides(tab:reduceMotion:)``).
  /// - Returns: Its opacity.
  static func detailsOpacity(isShown: Bool, slides: Bool) -> Double {
    guard !isShown, !slides else { return 1 }
    return 0
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
