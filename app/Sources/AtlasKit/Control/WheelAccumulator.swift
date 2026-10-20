/// Scrolling kept in fractions across sends: rounding each event would drop
/// slow trackpad movement before the helper ever saw it.
package struct WheelAccumulator: Sendable {
  /// 1e-9 of a line: the slack added away from zero before a sum is cut to whole lines, so a sum that floating
  /// point leaves a hair short of a whole line, as ten trackpad tenths add up to 0.9999999999999999, still sends
  /// that line. Far below any real scroll's fraction, so it never invents one.
  static let roundingSlack = 1e-9
  /// 10: a trackpad's points in one line, a wheel notch in the apps measured.
  static let pointsPerLine = 10.0
  /// 100: the most whole lines one send carries each way; the rest stays for the next.
  static let mostLines = 100.0

  var x = 0.0
  var y = 0.0

  /// Creates an accumulator holding nothing.
  package init() {}

  /// Drops whatever is held.
  package mutating func reset() {
    x = 0
    y = 0
  }

  /// Add one scroll event's deltas: points for a trackpad (`precise`),
  /// lines for a wheel. Positive moves content down and right, as AppKit
  /// has it; the sender turns that into Atlas's lines down the page. A
  /// line is a wheel notch, ``pointsPerLine`` points in the apps measured.
  ///
  /// - Parameters:
  ///   - dx: The horizontal delta.
  ///   - dy: The vertical delta.
  ///   - precise: Whether the deltas are points (a trackpad) rather than lines (a wheel).
  package mutating func add(dx: Double, dy: Double, precise: Bool) {
    let factor = precise ? 1 / Self.pointsPerLine : 1
    x += dx * factor
    y += dy * factor
  }

  /// The whole lines to send now (at most ``mostLines`` each way); the rest stays.
  ///
  /// - Returns: The whole lines each way.
  package mutating func take() -> (dx: Int, dy: Int) {
    func whole(_ value: Double) -> Int {
      let slack = value < 0 ? -Self.roundingSlack : Self.roundingSlack
      let lines = (value + slack)
        .rounded(.towardZero)
      return Int(max(-Self.mostLines, min(Self.mostLines, lines)))
    }
    let dx = whole(x)
    let dy = whole(y)
    x -= Double(dx)
    y -= Double(dy)
    return (dx, dy)
  }
}
