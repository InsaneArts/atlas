import SwiftUI

/// The helper's screen as one picture between the stage and the corner: the Screen tab's fitted picture and
/// the mini screen carry one geometry identity, so leaving the Screen for Activity or Apps shrinks the
/// picture into the corner and coming back grows it out, on ``Motion/zoom``. Both draw the window's one live
/// slot, so the side that arrives comes whole at the other's place and the side that leaves fades under or
/// over it: the two read as one picture moving, and the live layer is only moved, never drawn again for it.
/// Under Reduce Motion it carries none, and the corner crossfades.
struct ScreenZoom: ViewModifier {
  /// The identity the stage's picture and the mini screen share.
  static let id = "screen"

  /// The namespace the picture moves in, the window's.
  let namespace: Namespace.ID
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The view, carrying the screen's identity unless Reduce Motion is on.
  func body(content: Content) -> some View {
    if reduceMotion {
      content
    } else {
      content
        .matchedGeometryEffect(id: Self.id, in: namespace)
    }
  }

  /// Whether a switch from `old` to `new` moves the picture between the stage and the corner: one of them is the
  /// Screen and the other keeps the screen in its corner, the Screen shows its picture fitted (at actual size it
  /// scrolls, and fades as any pane), and Reduce Motion is off.
  ///
  /// - Parameters:
  ///   - old: The place left.
  ///   - new: The place shown.
  ///   - screen: What the Screen tab shows.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: Whether the picture moves.
  static func zooms(from old: ViewerPlace, to new: ViewerPlace, screen: ViewerScreenState, reduceMotion: Bool) -> Bool {
    guard !reduceMotion, screen.picture != nil, screen.actualSize == nil else { return false }
    return (old == .screen && new.keepsScreenInCorner) || (new == .screen && old.keepsScreenInCorner)
  }

  /// The switch's animation for the picture's move: ``Motion/zoom`` while it moves; none otherwise, as every other
  /// switch has, whose panes keep their own timing.
  ///
  /// - Parameter zooms: Whether the picture moves.
  /// - Returns: The animation, or nil.
  static func animation(zooms: Bool) -> Animation? {
    zooms ? Motion.zoom : nil
  }

  /// How opaque the stage's picture or the mini screen draws at either end of its transition: the side arriving comes
  /// whole while the picture moves, so nothing shows through it; every other arrival fades in, and the side leaving
  /// fades out.
  ///
  /// - Parameters:
  ///   - isArriving: Whether it comes in, rather than goes.
  ///   - isAway: Whether it is at its transition's far end: not yet in, or already gone.
  ///   - zooms: Whether the picture moves between the stage and the corner.
  /// - Returns: Its opacity.
  static func opacity(isArriving: Bool, isAway: Bool, zooms: Bool) -> Double {
    guard isAway else { return 1 }
    return isArriving && zooms ? 1 : 0
  }
}
