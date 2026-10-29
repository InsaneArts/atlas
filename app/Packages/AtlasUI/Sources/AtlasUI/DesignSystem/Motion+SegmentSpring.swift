import Foundation

extension Motion {
  /// A critically damped segmented thumb, sampled from its live position and velocity on every interruption.
  struct SegmentSpring: Equatable {
    /// The position at the handoff, in points.
    let origin: CGFloat
    /// The velocity at the handoff, in points per second.
    let velocity: CGFloat
    /// The destination's centre, in points.
    let target: CGFloat
    /// The handoff's clock time.
    let start: TimeInterval

    /// 0.3 s: the thumb's response, without bounce.
    static let response = Duration.state.seconds
    /// The angular frequency of the critically damped spring.
    static var frequency: Double { 2 * .pi / response * rate }
    /// The debug playback rate, shared with the rest of the motion lab.
    static var rate: Double {
      #if DEBUG
        Motion.playbackRate
      #else
        1
      #endif
    }
    /// The lifetime after which imperceptible residual motion is rounded to rest.
    static var lifetime: TimeInterval { response * 3 / rate }

    /// Samples the exact critical solution, preserving velocity even when the destination reverses.
    ///
    /// - Parameter time: The current clock time.
    /// - Returns: Position and velocity in points and points per second.
    func sample(at time: TimeInterval) -> (position: CGFloat, velocity: CGFloat) {
      let elapsed = max(.zero, time - start)
      guard elapsed < Self.lifetime else { return (target, .zero) }
      let displacement = origin - target
      let coefficient = velocity + Self.frequency * displacement
      let decay = exp(-Self.frequency * elapsed)
      return (
        target + (displacement + coefficient * elapsed) * decay,
        (velocity - Self.frequency * coefficient * elapsed) * decay
      )
    }
  }
}
