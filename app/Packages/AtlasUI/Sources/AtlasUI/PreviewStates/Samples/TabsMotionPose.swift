#if DEBUG
  import SwiftUI

  /// A held sample of the actual tap or drag-release interaction, from p = 0 through p = 1.
  struct TabsMotionPose {
    /// The release series starts between the middle and last segment, carrying a rightward velocity.
    let release: Bool
    /// The elapsed share of the spring's response; p = 1 holds the fully settled endpoint.
    let progress: Double

    /// The held clock time.
    var time: TimeInterval {
      progress >= 1
        ? Motion.SegmentSpring.lifetime : progress * Motion.SegmentSpring.response / Motion.SegmentSpring.rate
    }

    /// Builds a deterministic sample using the real interaction reducer and its measured segment widths.
    ///
    /// - Parameter geometry: The actual labels' geometry.
    /// - Returns: The live model at the start of this move, ready to sample at ``time``.
    func motion(in geometry: TabsGeometry) -> TabsMotion {
      var motion = TabsMotion()
      motion.update(frames: geometry.frames, selected: 0, at: .zero, reduced: false)
      guard let first = geometry.frames.first, let last = geometry.frames.last else { return motion }
      if release {
        let centre = first.midX + (last.midX - first.midX) * Self.releaseShare
        motion.press(at: first.midX, time: .zero)
        motion.drag(to: centre, reduced: false)
        _ = motion.release(at: centre, velocity: Self.releaseVelocity, time: .zero, reduced: false)
      } else {
        motion.press(at: last.midX, time: .zero)
        _ = motion.release(at: last.midX, velocity: .zero, time: .zero, reduced: false)
      }
      return motion
    }

    /// 0.8: the drag release's starting share of the whole track.
    private static let releaseShare = 0.8
    /// 180 pt/s: the pointer's speed as the sample drag releases.
    private static let releaseVelocity: CGFloat = 180
  }

  extension EnvironmentValues {
    /// A held segmented-control frame, for the motion lab's deterministic series only.
    @Entry var tabsMotionPose: TabsMotionPose? = nil
    /// A local Reduce Motion simulation in the segmented-control lab; nil follows the system.
    @Entry var tabsReducedMotion: Bool? = nil
  }
#endif
