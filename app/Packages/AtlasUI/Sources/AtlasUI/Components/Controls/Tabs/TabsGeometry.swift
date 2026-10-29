import Foundation

/// The measured segments; the thumb's width is a continuous function of its centre.
struct TabsGeometry: Equatable {
  /// Label frames, including their padding, in the track's coordinate space.
  var frames: [CGRect] = []

  /// The segment whose centre is closest; a tie favours the leading segment.
  ///
  /// - Parameter centre: The thumb's centre.
  /// - Returns: The nearest segment, or nil for an empty track.
  func nearest(to centre: CGFloat) -> Int? {
    frames.indices.min { abs(frames[$0].midX - centre) < abs(frames[$1].midX - centre) }
  }

  /// The segment under the thumb's centre, including half the gap on each side.
  ///
  /// - Parameter centre: The thumb's centre.
  /// - Returns: The preview segment.
  func segment(at centre: CGFloat) -> Int? {
    for index in frames.indices.dropLast() {
      if centre < (frames[index].maxX + frames[index + 1].minX) / 2 { return index }
    }
    return frames.indices.last
  }

  /// Interpolates the segment widths between their centres, retaining the end widths beyond the ends.
  ///
  /// - Parameter centre: The thumb's centre.
  /// - Returns: Its frame, with a continuously interpolated width.
  func thumb(at centre: CGFloat) -> CGRect {
    guard let first = frames.first, let last = frames.last else { return .zero }
    var width = centre <= first.midX ? first.width : last.width
    for index in frames.indices.dropLast() {
      let left = frames[index]
      let right = frames[index + 1]
      if centre >= left.midX, centre <= right.midX {
        let share = (centre - left.midX) / max(right.midX - left.midX, .ulpOfOne)
        width = left.width + (right.width - left.width) * share
        break
      }
    }
    return CGRect(x: centre - width / 2, y: first.minY, width: width, height: first.height)
  }

  /// Resists a drag beyond the first and last centres, or clamps it under Reduce Motion.
  ///
  /// - Parameters:
  ///   - centre: The unresisted pointer position.
  ///   - reduced: Whether Reduce Motion is on.
  /// - Returns: The resisted centre and its derivative, for the release velocity.
  func resisted(_ centre: CGFloat, reduced: Bool) -> (position: CGFloat, slope: CGFloat) {
    guard let first = frames.first, let last = frames.last else { return (centre, 1) }
    let bound = min(max(centre, first.midX), last.midX)
    let excess = centre - bound
    guard excess != .zero else { return (centre, 1) }
    guard !reduced else { return (bound, .zero) }
    let scale = TabsLayout.rubberBandLimit
    let divisor = 1 + abs(excess) / scale
    return (bound + excess / divisor, 1 / (divisor * divisor))
  }
}
