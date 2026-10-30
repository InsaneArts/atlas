#if DEBUG
  import CoreGraphics
  import SwiftUI

  /// One of the main window's transitions, to draw part of the way through as key frames: the window shows `from`,
  /// then `to`, in the animation the change runs in when the person asks for it, and each frame is the real transition
  /// held still (``Motion/Pose``) at a moment of its way (``Lead``): where its leading move has gone a share of the way,
  /// or a list's change stands in each of its beats, while everything else stands where it does at that instant, words
  /// that wait on the move included.
  public struct ViewerMotionPose: Identifiable {
    /// The shares of the leading move most key frames draw: its start, each quarter, and once it has settled.
    public static let quarters: [Double] = [0, 0.25, 0.5, 0.75, 1]

    /// The shares a closer look draws, such as a moment opening: its start, each tenth, and once it has settled.
    public static let tenths: [Double] = (0...10)
      .map { Double($0) / 10 }

    /// 1/120 s: how far a render moves the held moment at a time on its way to a frame, as finely as a fast display
    /// draws, so what one animation's end starts begins where it does on screen.
    public static let tick: TimeInterval = 1.0 / 120

    /// The transition's name, which names its frames' files.
    public let name: String
    /// What the window shows before the change.
    public let from: ViewerState
    /// What it shows after.
    public let to: ViewerState
    /// The window's size.
    public let size: CGSize
    /// What the frames are spaced along.
    let lead: Lead
    /// The shares of the way its key frames draw.
    public let steps: [Double]
    /// The animation the change runs in, as the view wraps the person's own change in it, such as opening a moment;
    /// nil for a change the window reads, such as a new step, which only the views' own animations move. It is made at
    /// the change, so a held moment poses it.
    public let animation: @MainActor () -> Animation?

    /// The transition's name, which identifies it.
    public var id: String { name }

    /// What a transition's key frames are spaced along.
    public enum Lead {
      /// One spring, the move that travels furthest: the frames stand at its quarters.
      case spring(Spring)
      /// A list's change in its three beats, rows leaving fading out, the rest moving on the spring, then rows
      /// arriving fading in (``Motion/listMove``, ``Motion/listIn``): the frames stand in each beat's middle.
      case list(Spring)
    }

    /// A transition from `from` to `to` in a window of `size`, its frames spaced along `lead`.
    ///
    /// - Parameters:
    ///   - name: Its name, which names its frames' files.
    ///   - from: What the window shows before the change.
    ///   - to: What it shows after.
    ///   - size: The window's size.
    ///   - lead: What the frames are spaced along.
    ///   - steps: The shares of the way its key frames draw; the quarters by default.
    ///   - animation: The animation the change runs in; none by default, as for a change the window reads.
    init(
      _ name: String,
      from: ViewerState,
      to: ViewerState,
      size: CGSize,
      lead: Lead,
      steps: [Double] = quarters,
      animation: @escaping @MainActor () -> Animation? = { nil }
    ) {
      self.name = name
      self.from = from
      self.to = to
      self.size = size
      self.lead = lead
      self.steps = steps
      self.animation = animation
    }

    /// A key frame's file name: the transition's, then the step as a share of a hundred, such as
    /// `motion-sidebar-collapse-050`.
    ///
    /// - Parameter step: The share of the leading move, from 0 to 1.
    /// - Returns: The name, without its extension.
    public func file(at step: Double) -> String {
      "motion-\(name)-" + String(format: "%03d", Int((step * 100).rounded()))
    }

    /// The moment, in seconds since the change, a key frame draws: for one spring, the moment it has gone `step` of
    /// the way, the change itself at 0 and the moment it settles at 1; for a list's change, each quarter a beat of
    /// it, the change at 0, the rows leaving half faded at 0.25, the rows moving half way at 0.5, the rows arriving
    /// half faded in at 0.75 and everything settled at 1, and between, a share of the way from one to the next; at
    /// the rate motion plays.
    ///
    /// - Parameter step: The share of the transition, from 0 to 1.
    /// - Returns: The moment.
    public func moment(at step: Double) -> TimeInterval {
      switch lead {
      case .spring(let spring):
        return Self.moment(along: spring, at: step) / Motion.playbackRate
      case .list(let spring):
        let out = Motion.Duration.swapOut.seconds
        let arrival = out + Motion.Duration.makeRoom.seconds
        let fadeIn = Motion.Duration.swapIn.seconds
        let beats = [
          .zero,
          out * Self.halfway,
          out + Self.moment(along: spring, at: Self.halfway),
          arrival + fadeIn * Self.halfway,
          max(arrival + fadeIn, out + spring.settlingDuration),
        ]
        let place = min(max(step, .zero), 1) * Double(beats.count - 1)
        let below = min(Int(place.rounded(.down)), beats.count - 2)
        let share = place - Double(below)
        return (beats[below] + (beats[below + 1] - beats[below]) * share) / Motion.playbackRate
      }
    }

    /// The time since it began at which `spring` has gone `step` of the way: none at 0, its settling time at 1.
    ///
    /// - Parameters:
    ///   - spring: The spring.
    ///   - step: The share of the way, from 0 to 1.
    /// - Returns: The time, in seconds, at the spring's own rate.
    private static func moment(along spring: Spring, at step: Double) -> TimeInterval {
      let settled = spring.settlingDuration
      guard step > .zero else { return .zero }
      guard step < 1 else { return settled }
      var early = TimeInterval.zero
      var late = settled
      // The spring never overshoots, so its share only grows with time and halving the span finds the moment.
      for _ in 0..<halvings {
        let middle = (early + late) / 2
        if spring.value(target: 1.0, time: middle) < step { early = middle } else { late = middle }
      }
      return late
    }

    /// 0.5: the share of a beat a key frame stands in the middle of.
    private static let halfway = 0.5

    /// 40: how often the span holding a moment is halved, far past a frame's precision.
    private static let halvings = 40
  }
#endif
