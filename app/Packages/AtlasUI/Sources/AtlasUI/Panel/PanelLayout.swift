import SwiftUI

/// The menu bar panel's own measures, which no other window shares (mac.py `panel_card`, v3 `.panel2`).
enum PanelLayout {
  /// The pieces of the panel that move when its state changes, in the order they move (v3 P7, X6): the word and its
  /// dot, then the line, the picture and the buttons.
  enum Piece: Int, CaseIterable, Sendable {
    /// The status's mark and word.
    case word
    /// The line under them.
    case line
    /// The picture's look.
    case picture
    /// The row of buttons.
    case buttons
  }

  /// 352: the panel's width.
  static let width: CGFloat = 352
  /// 12: the panel's inset on every side.
  static let inset = Spacing.m
  /// 10: between the header, the picture, Recent and the buttons.
  static let gap = Spacing.sPlus
  /// 328 by 205: the picture's frame, the width inside the inset at the screen's 16:10.
  static let picture = CGSize(width: 328, height: 205)
  /// 7: between the status's dot and its word (v3 `.wordrow`).
  static let wordGap: CGFloat = 7
  /// 40: Atlas's mark on the empty stage, by its width (v3 P6, `mark(40)`).
  static let emptyMark: CGFloat = 40
  /// 36: the empty stage's side inset, so its line wraps well inside the frame (v3 `.pic .empty`).
  static let emptyInset: CGFloat = 36
  /// 24: a Recent row's least height (v3 P5).
  static let recentRow: CGFloat = 24
  /// 8: the inset of whatever sits on the picture's corners: the caption, the pause button, the badge and the expand
  /// symbol (v3 P2).
  static let captionInset = Spacing.s
  /// 26: the round pause button on the picture, and the expand symbol's circle (v3 P2).
  static let pauseButton = ControlHeight.xsPlus
  /// The room the caption leaves at its right for the pause button: the button and its inset on both sides (v3
  /// `.caption`'s `right: 42`).
  static let captionTrailing = pauseButton + captionInset * 2
  /// The widest the caption may run: the picture less its inset on the left and the pause button's room.
  static let captionWidth = picture.width - captionInset - captionTrailing
  /// About 10: the expand symbol's size, 0.38 of its circle, as the round buttons draw theirs.
  static let cornerSymbol = pauseButton * 0.38
  /// 20: the orange badge on the picture while the helper needs the person (v3 P4).
  static let badge: CGFloat = 20
  /// 0.6: the badge pops in from this scale (v3 `.pic .badge`).
  static let badgeArrival: CGFloat = 0.6
  /// 1.5: control's accent rim, just outside the picture's edge (v3 P4).
  static let rimWidth = Stroke.emphasis
  /// 3: the soft ring of the accent outside the rim (v3 P4, `0 0 0 4.5px` less the rim).
  static let ringWidth: CGFloat = 3
  /// How far the rim reaches outside the picture.
  static let rimReach = rimWidth
  /// How far the ring reaches outside the picture: the rim and the ring.
  static let ringReach = rimWidth + ringWidth
  /// The rim's corners, the picture's grown by the rim.
  static let rimCorner = Radius.medium + rimReach
  /// The ring's corners, the picture's grown by the ring.
  static let ringCorner = Radius.medium + ringReach
  /// 1.015: the rim settles in from a little larger (v3 `.pic .rim`).
  static let rimArrival: CGFloat = 1.015
  /// 0.5: how much colour the picture keeps while it needs the person or is paused (v3 P4, `saturate(.5)`).
  static let dimSaturation = 0.5
  /// 0.3: the black over a dimmed picture, which leaves it at v3's `brightness(.7)`.
  static let dimShade = 0.3
  /// 0.55: the mark on the empty stage at rest (v3 `.pic .empty .mark`).
  static let markRest = 0.55
  /// 0.3: the low point of the connecting mark's breath (v3 P6).
  static let breathLow = 0.3
  /// 0.7: the high point of the connecting mark's breath (v3 P6).
  static let breathHigh = 0.7
  /// 0.5: the connecting mark held still under Reduce Motion, and in a render (v3 P6).
  static let breathStill = 0.5
  /// 1: how long the live row's blue dot stays before it cools away, in seconds (v3 P5).
  static let liveHold: TimeInterval = 1
  /// 0.03: the step between the cascade's pieces, in seconds (v3 P7).
  static let cascadeStep: TimeInterval = 0.03
  /// The panel's ground, rgb(30, 30, 33) (mac.py:366, v3 `--panel-glass`'s colour), drawn opaque under Reduce
  /// Transparency.
  static let ground = Color(red: 30 / 255, green: 30 / 255, blue: 33 / 255)
  /// 0.80: the ground's opacity over the popover's glass, so the material shows through (v3 P3).
  static let groundTint = 0.8
  /// 16: the panel's corners, where no popover holds it.
  static let corner: CGFloat = 16
  /// The panel's shadow, 0 20 50 black at 0.5 (mac.py:366): its colour.
  static let shadowColor = Color.black
    .opacity(0.5)
  /// 24: the shadow's radius, half the CSS 50 blur rounded to an even point, as ``Elevation`` halves them.
  static let shadowRadius: CGFloat = 24
  /// 20: how far the shadow falls below the panel.
  static let shadowDrop: CGFloat = 20
  /// How far past the card its shadow may spread: twice its radius and its drop.
  static let shadowRoom = shadowRadius * 2 + shadowDrop

  /// When a piece of the cascade starts after a state change: 30 milliseconds after the piece before it (v3 P7).
  ///
  /// - Parameter index: The piece's place in the cascade, from 0.
  /// - Returns: Its delay in seconds.
  static func cascadeDelay(index: Int) -> TimeInterval {
    cascadeStep * TimeInterval(index)
  }

  /// How a piece moves to a new state: ``Motion/state`` after its place in the cascade, or the short fade
  /// ``Motion/reduced`` at once under Reduce Motion.
  ///
  /// - Parameters:
  ///   - piece: The piece.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: Its animation.
  static func cascade(_ piece: Piece, reduceMotion: Bool) -> Animation {
    Motion.animation(
      Motion.state.delay(cascadeDelay(index: piece.rawValue)),
      reduceMotion: reduceMotion,
      reduced: Motion.reduced
    )
  }

  /// How Stop comes and goes in its slot (v3 D11, P7): growing in on ``Motion/arrive`` and shrinking away on
  /// ``Motion/leave``, as every slot does, but fourth in the cascade with the buttons, never ahead of the word; the
  /// short fade ``Motion/reduced`` at once under Reduce Motion.
  ///
  /// - Parameters:
  ///   - isShown: Whether Stop shows now.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The slot's animation.
  static func slot(isShown: Bool, reduceMotion: Bool) -> Animation {
    let base = isShown ? Motion.arrive : Motion.leave
    return Motion.animation(
      base.delay(cascadeDelay(index: Piece.buttons.rawValue)),
      reduceMotion: reduceMotion,
      reduced: Motion.reduced
    )
  }

  /// The ground's opacity: the tint over the popover's glass, or whole under Reduce Transparency (v3 P3).
  ///
  /// - Parameter reduceTransparency: Whether the Reduce Transparency setting is on.
  /// - Returns: The opacity.
  static func groundOpacity(reduceTransparency: Bool) -> Double {
    reduceTransparency ? 1 : groundTint
  }
}
