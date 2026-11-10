import CoreGraphics

/// How large an alert card draws as it comes and goes. The alert window's settles from a little larger; a window's
/// alert layer grows in from a little smaller and leaves the same way, in place.
enum AlertCardScale {
  /// 1.04: how large the card starts before it settles, small enough that settling reads as arriving, not zooming.
  static let arriving: CGFloat = 1.04
  /// 0.97: how small the card ends as it leaves, and, in a window's alert layer, how small it starts.
  static let leaving: CGFloat = 0.97

  /// The card's scale as it arrives: a little larger until it settles, never under Reduce Motion, where it only
  /// fades in.
  ///
  /// - Parameters:
  ///   - settled: Whether it has settled.
  ///   - reduceMotion: Whether Reduce Motion is on.
  /// - Returns: The scale.
  static func arrival(settled: Bool, reduceMotion: Bool) -> CGFloat {
    settled || reduceMotion ? 1 : arriving
  }
}
