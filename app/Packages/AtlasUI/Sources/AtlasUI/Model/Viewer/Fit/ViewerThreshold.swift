import CoreGraphics

/// A size the window's content gives way at, with room either side of it, so a window held near it never flickers
/// between the two layouts: a measure rising from below crosses it only at ``high``, and one falling from above only
/// under ``low``; between the two, whatever stood stays.
struct ViewerThreshold: Equatable, Sendable {
  /// What a measure falls under to leave the layout above.
  let low: CGFloat
  /// What a measure rises to to take the layout above.
  let high: CGFloat

  /// A threshold the layout above leaves under `value`, coming back only `room` past it.
  ///
  /// - Parameters:
  ///   - value: Where the layout above gives way.
  ///   - room: How far past it a growing measure comes back.
  init(fallsUnder value: CGFloat, room: CGFloat) {
    low = value
    high = value + room
  }

  /// A threshold the layout above is taken at `value`, given up only `room` under it.
  ///
  /// - Parameters:
  ///   - value: Where the layout above is taken.
  ///   - room: How far under it a shrinking measure gives it up.
  init(risesAt value: CGFloat, room: CGFloat) {
    low = value - room
    high = value
  }

  /// Whether `value` stands in the layout above: past ``high``, or past ``low`` while it stood there already.
  ///
  /// - Parameters:
  ///   - value: The measure now.
  ///   - wasAbove: Whether the layout above stood until now.
  /// - Returns: Whether it stands now.
  func isAbove(_ value: CGFloat, wasAbove: Bool) -> Bool {
    value >= (wasAbove ? low : high)
  }
}
