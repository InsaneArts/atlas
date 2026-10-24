import CoreGraphics

/// How large an alert card draws as it comes and goes (v3 A6): settling from a little larger, leaving a little
/// smaller. The window layer and the alert window both read it.
enum AlertCardScale {
  /// 1.06: how large the card starts before it settles.
  static let arriving: CGFloat = 1.06
  /// 0.97: how small the card ends as it leaves.
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
