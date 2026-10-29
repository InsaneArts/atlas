import SwiftUI

/// The menu bar panel's own measures, which no other window shares.
enum PanelLayout {
  /// 352: the panel's width.
  static let width: CGFloat = 352
  /// 12: the panel's inset on every side.
  static let inset = Spacing.m
  /// 10: between the header, the picture, Recent or Sessions, and the buttons.
  static let gap = Spacing.sPlus
  /// 328 by 205: the picture's frame while there is no screen to show, the width inside the inset at 16:10.
  static let picture = CGSize(width: 328, height: 205)
  /// 4:3 to 16:9: the screen shapes the picture's frame takes on; a screen outside them is letterboxed.
  static let pictureAspects: ClosedRange<CGFloat> = (4.0 / 3.0)...(16.0 / 9.0)
  /// 7: between the status's dot and its word.
  static let wordGap: CGFloat = 7
  /// 40: Atlas's mark on the empty stage, by its width.
  static let emptyMark: CGFloat = 40
  /// 36: the empty stage's side inset, so its line wraps well inside the frame.
  static let emptyInset: CGFloat = 36
  /// 24: the sessions' "+N more" row's least height.
  static let moreRow: CGFloat = 24
  /// 28: one of Recent's rows, its 20-point icon with room above and below.
  static let recentRow = ControlHeight.s
  /// 20: the icon that leads each of Recent's rows: the app's, Atlas's or the person's.
  static let recentIcon: CGFloat = 20
  /// 44: a session row's least height, its name over its line, as the main window's sessions column's.
  static let sessionRow = ControlHeight.xl
  /// 28: a session row's icon, the main window's sessions column's.
  static let sessionIcon = SessionIcon.rowSide
  /// 3: the most session rows the panel lists, as Recent lists three actions.
  static let mostSessions = 3
  /// A session row's inset round its content, inside its hover fill: 4 before the icon, so it stands 16 from the
  /// panel's edge as the text above it does, 8 after the state, clear of the fill's curve, and 4 above and below, so
  /// the name over its line, 33 high, keeps the row at 44 with the icon centred 8 from its edges.
  static let sessionInset = EdgeInsets(top: Spacing.xs, leading: Spacing.xs, bottom: Spacing.xs, trailing: Spacing.s)
  /// 14: a session row's hover corner, the card's less the panel's inset, so the two curves run side by side.
  static let sessionCorner = Radius.nested(in: corner, inset: inset)
  /// 4: the header's, a heading's and a Recent row's inset inside the panel's, so their words start 16 from its edge,
  /// in line with the session rows' icons.
  static let textInset = Spacing.xs
  /// 10: between a session row's icon and its words.
  static let sessionGap = Spacing.sPlus
  /// Where a session row's words start: past its inset, its icon and the gap after it, so the row under the sessions
  /// lines up with their names.
  static let sessionText = sessionInset.leading + sessionIcon + sessionGap
  /// 8: the inset of whatever sits on the picture's corners: the caption, the pause button, the badge and the expand
  /// symbol.
  static let captionInset = Spacing.s
  /// 26: the round pause button on the picture, and the expand symbol's circle.
  static let pauseButton = ControlHeight.xsPlus
  /// The room the caption leaves at its right for the pause button: the button and its inset on both sides.
  static let captionTrailing = pauseButton + captionInset * 2
  /// The widest the caption may run: the picture less its inset on the left and the pause button's room.
  static let captionWidth = picture.width - captionInset - captionTrailing
  /// About 10: the expand symbol's size, 0.38 of its circle, as the round buttons draw theirs.
  static let cornerSymbol = pauseButton * 0.38
  /// 20: the orange badge on the picture while the helper needs the person.
  static let badge: CGFloat = 20
  /// 0.6: the badge pops in from this scale.
  static let badgeArrival: CGFloat = 0.6
  /// 1.5: control's accent rim, just outside the picture's edge.
  static let rimWidth = Stroke.emphasis
  /// 3: the soft ring of the accent outside the rim.
  static let ringWidth: CGFloat = 3
  /// How far the rim reaches outside the picture.
  static let rimReach = rimWidth
  /// How far the ring reaches outside the picture: the rim and the ring.
  static let ringReach = rimWidth + ringWidth
  /// 14: the picture's corners, the card's less the panel's inset, so the two curves run side by side.
  static let pictureCorner = Radius.nested(in: corner, inset: inset)
  /// The rim's corners, the picture's grown by the rim.
  static let rimCorner = pictureCorner + rimReach
  /// The ring's corners, the picture's grown by the ring.
  static let ringCorner = pictureCorner + ringReach
  /// 1.015: the rim settles in from a little larger.
  static let rimArrival: CGFloat = 1.015
  /// 0.5: how much colour the picture keeps while it needs the person or is paused.
  static let dimSaturation = 0.5
  /// 0.3: the black over a dimmed picture, which leaves it at 70% brightness.
  static let dimShade = 0.3
  /// 0.55: the mark on the empty stage at rest.
  static let markRest = 0.55
  /// 0.3: the low point of the connecting mark's breath.
  static let breathLow = 0.3
  /// 0.7: the high point of the connecting mark's breath.
  static let breathHigh = 0.7
  /// 0.5: the connecting mark held still under Reduce Motion, and in a render.
  static let breathStill = 0.5
  /// 1: how long the live row's blue dot stays before it cools away, in seconds.
  static let liveHold: TimeInterval = 1
  /// The panel's ground, #F6F6F8 under Light and rgb(30, 30, 33) under Dark, opaque, so the popover's own material
  /// never shows through it.
  static let ground = Color(light: .rgb(0xF6F6F8), dark: .rgb(0x1E1E21))
  /// 26: the panel's corners, where no popover holds it, a modal surface's: the panel's inset and the round
  /// controls' half height inside it, so what sits at the inset nests in the corner.
  static let corner = Radius.extraLarge
  /// The panel's shadow, 0 20 50 in a surface's shadow colour (``Color/dropShadow``): its colour.
  static let shadowColor = Color.dropShadow
  /// 24: the shadow's radius, half the CSS 50 blur rounded to an even point, as ``Elevation`` halves them.
  static let shadowRadius: CGFloat = 24
  /// 20: how far the shadow falls below the panel.
  static let shadowDrop: CGFloat = 20
  /// How far past the card its shadow may spread: twice its radius and its drop.
  static let shadowRoom = shadowRadius * 2 + shadowDrop

  /// How the panel moves to a new state: the word, the line, the picture's look and the buttons all together, easing
  /// out over ``Motion/Duration/fade``. Only colour, opacity and the rim's fade change, so Reduce Motion keeps it, and
  /// nothing bounces.
  static var change: Animation { Motion.fade }

  /// The picture's frame for a screen of a shape: the panel's width, and as tall as the screen's shape makes it, so a
  /// screen of any usual shape fills its frame with no black at its sides; the 16:10 frame while there is none.
  ///
  /// - Parameter aspect: The screen's width over its height, or nil with no screen.
  /// - Returns: The frame, its height in whole points.
  static func pictureSize(aspect: CGFloat?) -> CGSize {
    guard let aspect, aspect.isFinite, aspect > 0 else { return picture }
    let fitted = min(max(aspect, pictureAspects.lowerBound), pictureAspects.upperBound)
    return CGSize(width: picture.width, height: (picture.width / fitted).rounded())
  }
}
