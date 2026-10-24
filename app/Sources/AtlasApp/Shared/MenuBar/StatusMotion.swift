import AtlasUI

/// What the menu bar item may move as it shows a state (v3 M4, X14).
enum StatusMotion: Equatable {
  /// Everything: the badge's pop and crossfades, the orbit's dim and the breath.
  case all
  /// Only a breath that runs: a menu is open, so a new state swaps in at once and nothing moves in the frame its
  /// tracking starts (v3 R16); a running breath keeps running, and none starts until the menu closes.
  case breath
  /// Nothing: Reduce Motion is on, so a new state swaps in at once and nothing breathes.
  case still

  /// What may move now.
  ///
  /// - Parameters:
  ///   - reduceMotion: Whether Reduce Motion is on.
  ///   - menuOpen: Whether a menu is open.
  /// - Returns: Nothing under Reduce Motion, only the breath while a menu is open, otherwise everything.
  static func now(reduceMotion: Bool, menuOpen: Bool) -> StatusMotion {
    guard Motion.loops(reduceMotion: reduceMotion) else { return .still }
    return menuOpen ? .breath : .all
  }
}
