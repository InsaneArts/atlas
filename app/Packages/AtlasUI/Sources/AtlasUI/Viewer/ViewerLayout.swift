import SwiftUI

/// The main window's own measures, which no other window shares.
enum ViewerLayout {
  /// 8: a pane's inset from the window's edges, so its corners nest in the window's.
  static let paneInset = Spacing.s
  /// 8: a pane's corner, the window's less the pane's inset, so the two curves run side by side.
  static let paneRadius = Radius.nested(in: Radius.window, inset: paneInset)
  /// 20: from the window's leading edge to the window buttons.
  static let lightsInset: CGFloat = 20
  /// 52: the three window buttons, 12 across and 8 apart.
  static let lightsWidth: CGFloat = 52
  /// 1.5 by 12: the caret after the text being typed, on the footer's line.
  static let caret = CGSize(width: 1.5, height: 12)
  /// 4: how far the pointer moves on the mini screen before a press becomes a drag; less is a click, back to the
  /// screen.
  static let miniDragStart: CGFloat = 4
  /// 0.96: how small the mini screen starts as it fades in without growing from the stage, as with the first picture.
  static let miniArrival: CGFloat = 0.96
  /// The spring the mini screen lands on in the corner it was thrown toward: 0.4 seconds with a little bounce, the one
  /// bounce in the window, since the person threw it.
  static let miniThrown = Spring(duration: 0.4, bounce: 0.1)
  /// 16 by 10: a screenshot's tile, and the screen's shape where a sample needs one.
  static let placeholderAspect = CGSize(width: 16, height: 10)
  /// 16: a pane's inset at its sides, which its header, rows and groups keep.
  static let swapInset = Spacing.l
  /// 26: an app's icon at the head of an Activity moment.
  static let appIcon = IconSize.mPlus
  /// 280: the least width a screenshot's tile takes before the grid drops a column, so a full window holds
  /// four and a narrow one two.
  static let shotLeastWidth: CGFloat = 280
  /// 14: between the Screenshots pane's tiles, across and down.
  static let shotGap = Spacing.mPlus
  /// 10: above the Screenshots pane's first heading.
  static let shotHeadingTop = Spacing.sPlus
  /// 16: above each later heading, past the gap under the row before it, so a group reads apart from the last.
  static let shotHeadingAbove = Spacing.l
  /// 6: between a screenshot and its time under it.
  static let shotTimeGap = Spacing.xsPlus
  /// 0.72: the shade over the window behind a screenshot shown large; the window behind is not blurred, the shade is
  /// enough.
  static let shotScrim = 0.72
  /// The shade's and the large view's bar's fade, in and out: ``Motion/fade``, which only changes opacity, so
  /// Reduce Motion keeps it.
  static var shotFade: Animation { Motion.fade }
  /// 26: from the window's bottom to the large screenshot's bar.
  static let shotBarInset: CGFloat = 26
  /// 0.36: how dark the picture goes outside a window shown from the Apps pane.
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

  /// How the mini screen lands in its corner once let go of: on ``miniThrown`` from wherever the drag left it, going
  /// on at the speed it was thrown; at once under Reduce Motion.
  ///
  /// - Parameters:
  ///   - velocity: Its speed toward the corner as it was let go, as a share of the way left each second.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The animation, or nil.
  static func miniLanding(velocity: Double, reduceMotion: Bool) -> Animation? {
    Motion.layout(.interpolatingSpring(miniThrown, initialVelocity: velocity), reduceMotion: reduceMotion)
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
