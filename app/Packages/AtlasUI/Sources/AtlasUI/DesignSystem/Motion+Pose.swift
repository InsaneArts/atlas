#if DEBUG
  import Foundation
  import os

  extension Motion {
    /// A moment every animation stands still at, so a debug render can draw a transition part of the way through:
    /// while a moment is held, each animation made through ``Motion/paced(_:)`` is drawn as it stands that long after
    /// it began (``PosedAnimation``), the real one, not a drawing of it, so the frame shows what the window shows at
    /// that instant. A render holds the moment at the change, then moves it on in small steps up to the moment it
    /// draws, so an animation chained on another's completion, such as words fading in once a slide settles, begins
    /// where the first ended. With no moment held, every animation runs on the clock, as in the app.
    public enum Pose {
      /// The moment held, in seconds since the change being posed; nil while none is.
      private static let held = OSAllocatedUnfairLock<TimeInterval?>(initialState: nil)

      /// The moment held, in seconds since the change being posed, or nil while animations run on the clock.
      public static var moment: TimeInterval? {
        held.withLock { $0 }
      }

      /// Holds every animation at `moment`, or lets them run on the clock again with nil.
      ///
      /// - Parameter moment: Seconds since the change being posed, or nil.
      public static func hold(at moment: TimeInterval?) {
        held.withLock { $0 = moment }
      }
    }
  }
#endif
