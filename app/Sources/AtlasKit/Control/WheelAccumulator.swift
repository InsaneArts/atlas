/// Scrolling kept in fractions across sends: rounding each event would drop
/// slow trackpad movement before the helper ever saw it.
package struct WheelAccumulator: Sendable {
  /// 1e-9 of a line: the slack added away from zero before a sum is cut to whole lines, so a sum that floating
  /// point leaves a hair short of a whole line, as ten trackpad tenths add up to 0.9999999999999999, still sends
  /// that line. Far below any real scroll's fraction, so it never invents one.
  static let roundingSlack = 1e-9

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
  /// line is a wheel notch, about ten points in the apps measured.
  ///
  /// - Parameters:
  ///   - dx: The horizontal delta.
  ///   - dy: The vertical delta.
  ///   - precise: Whether the deltas are points (a trackpad) rather than lines (a wheel).
  package mutating func add(dx: Double, dy: Double, precise: Bool) {
    let factor = precise ? 1.0 / 10 : 1
    x += dx * factor
    y += dy * factor
  }

  /// The whole lines to send now (at most 100 each way); the rest stays.
  ///
  /// - Returns: The whole lines each way.
  package mutating func take() -> (dx: Int, dy: Int) {
    func whole(_ value: Double) -> Int {
      Int(
        max(
          -100,
          min(
            100,
            (value + (value < 0 ? -Self.roundingSlack : value > 0 ? Self.roundingSlack : 0)).rounded(.towardZero)
          )
        )
      )
    }
    let dx = whole(x)
    let dy = whole(y)
    x -= Double(dx)
    y -= Double(dy)
    return (dx, dy)
  }
}
