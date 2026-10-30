import SwiftUI

extension ViewerLayout {
  /// How the sidebar hides and shows, the card's edge and the header's inset moving with it, and how it slides over
  /// the card: on ``Motion/slide``; under Reduce Motion nothing slides, and what changes fades over
  /// ``Motion/Duration/reduced`` on its opacity transition.
  ///
  /// - Parameter reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The animation.
  static func sidebarMotion(reduceMotion: Bool) -> Animation? {
    Motion.layout(Motion.slide, reduceMotion: reduceMotion)
  }

  /// How the footer's capsules narrow into circles and open again as the card's width crosses its threshold, never on
  /// a press: on ``Motion/state``; under Reduce Motion they change at once.
  ///
  /// - Parameter reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The animation, or nil for at once.
  static func footerFoldMotion(reduceMotion: Bool) -> Animation? {
    Motion.layout(Motion.state, reduceMotion: reduceMotion)
  }

  /// How Ended's chevron turns and its rows' room opens as it opens or folds: on ``Motion/state``; under Reduce Motion,
  /// where the chevron only crossfades, geometry changes at once.
  ///
  /// - Parameter reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The animation.
  static func endedMotion(reduceMotion: Bool) -> Animation? {
    Motion.layout(Motion.state, reduceMotion: reduceMotion)
  }
}
