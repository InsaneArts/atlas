import AtlasService
import AtlasUI

/// Whose rows Activity lists, as the window's choice of sessions picks them.
extension ActivityScope {
  /// The scope a choice of sessions picks: every row while every session's work shows, else the chosen session's.
  ///
  /// - Parameter choice: Whose work the window shows.
  init(_ choice: ViewerSessionChoice) {
    self = choice.key.map(Self.session) ?? .all
  }
}
