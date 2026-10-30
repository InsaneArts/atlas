import SwiftUI

extension Motion {
  /// An animation at the rate motion plays: itself, unless a debug build was launched with `ATLAS_MOTION_RATE`, which
  /// plays it at that rate (``playbackRate``), such as 0.25 for a quarter of its speed, so a transition can be watched
  /// frame by frame; and while a debug render holds a moment (``Pose``), it stands still there. A release build never
  /// reads the variable and never changes an animation.
  ///
  /// - Parameter animation: The animation, as its token defines it.
  /// - Returns: `animation`, at the playback rate.
  static func paced(_ animation: Animation) -> Animation {
    #if DEBUG
      let rated = playbackRate == 1 ? animation : animation.speed(playbackRate)
      guard Pose.moment != nil else { return rated }
      return Animation(PosedAnimation(base: rated))
    #else
      animation
    #endif
  }

  /// A wait that sits between animations at the rate motion plays, such as a line's hold before the next replaces it,
  /// so a slowed debug build keeps the same rhythm: the step's length, divided by ``playbackRate`` in a debug build.
  ///
  /// - Parameter duration: The step of the duration scale.
  /// - Returns: How long to wait, in seconds.
  static func paced(_ duration: Duration) -> TimeInterval {
    #if DEBUG
      duration.seconds / playbackRate
    #else
      duration.seconds
    #endif
  }

  #if DEBUG
    /// The environment variable a debug build reads its playback rate from.
    static let playbackRateVariable = "ATLAS_MOTION_RATE"

    /// How fast every animation plays in this debug build: the rate in ``playbackRateVariable``, read once at first
    /// use, or 1. An immutable `static let` is initialised once, lazily and safely, so any thread may read it.
    static let playbackRate = rate(from: ProcessInfo.processInfo.environment)

    /// The playback rate an environment asks for: its ``playbackRateVariable`` as a positive, finite number, or 1
    /// when the variable is missing or holds anything else.
    ///
    /// - Parameter environment: The process's environment variables.
    /// - Returns: The rate, 1 for an animation's own speed.
    static func rate(from environment: [String: String]) -> Double {
      guard
        let words = environment[playbackRateVariable],
        let rate = Double(words),
        rate.isFinite,
        rate > 0
      else { return 1 }
      return rate
    }
  #endif
}
