import SwiftUI

/// The menu bar panel's own measures, which no other window shares.
enum PanelLayout {
  /// 352: the panel's width.
  static let width: CGFloat = 352
  /// 12: the panel's inset on every side.
  static let inset = Spacing.m
  /// 10: between the header, the picture, Recent or Sessions, and the buttons.
  static let gap = Spacing.sPlus
  /// 328 by 205: the picture's frame, the width inside the inset at the screen's 16:10.
  static let picture = CGSize(width: 328, height: 205)
  /// 7: between the status's dot and its word.
  static let wordGap: CGFloat = 7
  /// 40: Atlas's mark on the empty stage, by its width.
  static let emptyMark: CGFloat = 40
  /// 36: the empty stage's side inset, so its line wraps well inside the frame.
  static let emptyInset: CGFloat = 36
  /// 24: a Recent row's least height.
  static let recentRow: CGFloat = 24
  /// 40: a session row's least height, its name over its line.
  static let sessionRow: CGFloat = 40
  /// 24: a session row's icon.
  static let sessionIcon = IconSize.m
  /// 3: the most session rows the panel lists, as Recent lists three actions.
  static let mostSessions = 3
  /// 4: a session row's inset round its content, inside its hover fill.
  static let sessionInset = Spacing.xs
  /// 10: between a session row's icon and its words.
  static let sessionGap = Spacing.sPlus
  /// Where a session row's words start: past its inset, its icon and the gap after it, so the row under the sessions
  /// lines up with their names.
  static let sessionText = sessionInset + sessionIcon + sessionGap
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
  /// The rim's corners, the picture's grown by the rim.
  static let rimCorner = Radius.medium + rimReach
  /// The ring's corners, the picture's grown by the ring.
  static let ringCorner = Radius.medium + ringReach
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
  /// The panel's ground, rgb(30, 30, 33), drawn opaque under Reduce
  /// Transparency.
  static let ground = Color(red: 30 / 255, green: 30 / 255, blue: 33 / 255)
  /// 0.80: the ground's opacity over the popover's glass, so the material shows through.
  static let groundTint = 0.8
  /// 16: the panel's corners, where no popover holds it.
  static let corner: CGFloat = 16
  /// The panel's shadow, 0 20 50 black at 0.5: its colour.
  static let shadowColor = Color.black
    .opacity(0.5)
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

  /// The ground's opacity: the tint over the popover's glass, or whole under Reduce Transparency.
  ///
  /// - Parameter reduceTransparency: Whether the Reduce Transparency setting is on.
  /// - Returns: The opacity.
  static func groundOpacity(reduceTransparency: Bool) -> Double {
    reduceTransparency ? 1 : groundTint
  }
}
