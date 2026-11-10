import SwiftUI

/// The main window's own measures, which no other window shares.
enum ViewerLayout {
  /// 8: a pane's inset from the window's edges, so its corners nest in the window's.
  static let paneInset = Spacing.s
  /// 8: a pane's corner, the window's less the pane's inset, so the two curves run side by side.
  static let paneRadius = Radius.nested(in: Radius.window, inset: paneInset)
  /// 20: from the window's leading edge to the window buttons (``WindowChrome/lightsInset``).
  static let lightsInset = WindowChrome.lightsInset
  /// 52: the three window buttons, 12 across and 8 apart (``WindowChrome/lightsWidth``).
  static let lightsWidth = WindowChrome.lightsWidth
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
  /// 16 by 10: the screen's shape where a picture or a sample has none of its own.
  static let placeholderAspect = CGSize(width: 16, height: 10)
  /// 16: a pane's inset at its sides, which its header, rows and groups keep.
  static let swapInset = Spacing.l
  /// 0.72: the shade over the window behind a run replayed; the window behind is not blurred, the shade is enough.
  static let replayScrim = 0.72
  /// The replay's shade's fade, in and out: ``Motion/fade``, which only changes opacity, so Reduce Motion keeps it.
  static var replayFade: Animation { Motion.fade }
  /// 14: how much the last picture blurs behind Connecting's words.
  static let lastFrameBlur: CGFloat = 14
  /// 0.6: how much colour the last picture keeps behind them.
  static let lastFrameSaturation = 0.6
  /// 0.55: the black laid over it, so it keeps 0.45 of its brightness, as CSS `brightness(.45)` multiplies.
  static let lastFrameShade = 0.55
  /// 1.04: how much larger the blurred picture draws than the stage, so its blurred edges fall outside it.
  static let lastFrameScale: CGFloat = 1.04
  /// 1040 by 650: the size the previews draw a pane at.
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
}
