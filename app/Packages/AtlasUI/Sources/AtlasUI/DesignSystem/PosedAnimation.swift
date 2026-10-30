#if DEBUG
  import SwiftUI

  /// An animation drawn at the moment a debug render holds (``Motion/Pose``) rather than on the clock: the animation
  /// it wraps, as it stands that long after it began, which is the moment held when it first ran, so an animation made
  /// before the change, such as a row's way out, begins with the change that runs it. Once the moment passes its end it
  /// is done, so what waits on its completion follows; and once the moment is let go it ends where it was going.
  struct PosedAnimation: CustomAnimation {
    /// The animation, as its token defines it.
    let base: Animation

    /// How far the animation has gone at the moment held: none as it begins, then `base`'s own progress, and done
    /// once `base` is or the moment is let go.
    ///
    /// - Parameters:
    ///   - value: The whole change the animation makes.
    ///   - time: The time on the clock, which a held moment stands in for.
    ///   - context: The animation's context, which keeps the moment it began and is passed on to `base`.
    /// - Returns: How much of `value` it has made, or nil once it is done.
    func animate<V: VectorArithmetic>(value: V, time: TimeInterval, context: inout AnimationContext<V>) -> V? {
      guard let moment = Motion.Pose.moment else { return nil }
      let start = context.state[PosedStart.self] ?? moment
      context.state[PosedStart.self] = start
      let elapsed = moment - start
      guard elapsed > .zero else { return .zero }
      return base.animate(value: value, time: elapsed, context: &context)
    }

    /// How fast the animation goes at the moment held, as `base` reckons it.
    ///
    /// - Parameters:
    ///   - value: The whole change the animation makes.
    ///   - time: The time on the clock, which a held moment stands in for.
    ///   - context: The animation's context, passed on to `base`.
    /// - Returns: Its velocity, if `base` has one.
    func velocity<V: VectorArithmetic>(value: V, time: TimeInterval, context: AnimationContext<V>) -> V? {
      guard let moment = Motion.Pose.moment else { return nil }
      let start = context.state[PosedStart.self] ?? moment
      return base.velocity(value: value, time: max(moment - start, .zero), context: context)
    }
  }
#endif
