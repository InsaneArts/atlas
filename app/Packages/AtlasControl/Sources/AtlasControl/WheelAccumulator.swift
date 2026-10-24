/// Scrolling kept in fractions across sends: rounding each event would drop
/// slow trackpad movement before the helper ever saw it.
public struct WheelAccumulator: Sendable {
  /// 1e-9 of a line: the slack added away from zero before a sum is cut to whole lines, so a sum that floating
  /// point leaves a hair short of a whole line, as ten trackpad tenths add up to 0.9999999999999999, still sends
  /// that line. Far below any real scroll's fraction, so it never invents one.
  static let roundingSlack = 1e-9
  /// 10: a trackpad's points in one line, a wheel notch in the apps measured.
  static let pointsPerLine = 10.0
  /// 100: the most whole lines one send carries each way; the rest stays for the next.
  static let mostLines = 100.0

  /// The horizontal scroll not yet sent, in lines.
  var x = 0.0
  /// The vertical scroll not yet sent, in lines.
  var y = 0.0

  /// Whether at least one whole line waits, as one that ``take()`` capped leaves.
  public var holdsWholeLines: Bool { Self.whole(x) != 0 || Self.whole(y) != 0 }

  /// Creates an accumulator holding nothing.
  public init() {}

  /// Drops whatever is held.
  public mutating func reset() {
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
  public mutating func add(dx: Double, dy: Double, precise: Bool) {
    let factor = precise ? 1 / Self.pointsPerLine : 1
    x += dx * factor
    y += dy * factor
  }

  /// A new scroll gesture begins: the fraction of a line the last one left goes, so it never rides along with
  /// scrolling the person did later and elsewhere. Whole lines not yet sent stay, as the person did scroll them.
  public mutating func beginGesture() {
    x = Self.whole(x)
    y = Self.whole(y)
  }

  /// The whole lines to send now (at most ``mostLines`` each way); the rest stays.
  ///
  /// - Returns: The whole lines each way.
  public mutating func take() -> (dx: Int, dy: Int) {
    func capped(_ value: Double) -> Int { Int(max(-Self.mostLines, min(Self.mostLines, Self.whole(value)))) }
    let dx = capped(x)
    let dy = capped(y)
    x -= Double(dx)
    y -= Double(dy)
    return (dx, dy)
  }

  /// The whole lines in `value`, towards zero, with ``roundingSlack``.
  ///
  /// - Parameter value: Lines, with any fraction.
  /// - Returns: Its whole lines.
  private static func whole(_ value: Double) -> Double {
    let slack = value < 0 ? -roundingSlack : roundingSlack
    return (value + slack)
      .rounded(.towardZero)
  }
}
