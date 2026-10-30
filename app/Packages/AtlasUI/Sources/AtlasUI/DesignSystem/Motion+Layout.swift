import SwiftUI

extension Motion {
  /// The single rule for structural transactions: Reduce Motion commits geometry at once. Fades belong only on
  /// opacity transitions or scoped opacity modifiers, never on a transaction that changes layout.
  ///
  /// - Parameters:
  ///   - animation: The layout animation requested by the caller, including an inherited animation.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: No animation under Reduce Motion; otherwise the requested animation.
  static func layout(_ animation: Animation?, reduceMotion: Bool) -> Animation? {
    reduceMotion ? nil : animation
  }
}
