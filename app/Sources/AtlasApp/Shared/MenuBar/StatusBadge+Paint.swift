import CoreGraphics

extension StatusBadge {
  /// How a badge is painted: a disc or a square filled with no stroke, a ring stroked with a clear fill.
  struct Paint {
    /// The fill's colour; clear for a ring.
    let fill: CGColor?
    /// The stroke's colour; clear for a disc or a square.
    let stroke: CGColor?
  }
}
