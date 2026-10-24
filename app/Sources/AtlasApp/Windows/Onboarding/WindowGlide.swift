import AppKit
import AtlasUI

/// Moves the setup window to a new place on the screen: a glide that leaves softly, travels quickly and lands softly
/// over ``Motion/Duration/glide``, or at once with Reduce Motion. It counts the glides running, so nothing else moves
/// the window meanwhile.
final class WindowGlide {
  /// The glide's curve, its control points: an ease in and out steeper than AppKit's own.
  static let curve = (x1: Float(0.65), y1: Float(0), x2: Float(0.35), y2: Float(1))

  /// The window it moves, which owns this glide and so outlives it.
  private unowned let window: NSWindow
  /// Whether Reduce Motion is on, read as each move starts.
  private let reduceMotion: @MainActor () -> Bool
  /// Glides running now.
  private var running = 0
  /// Whether a glide runs now.
  var isGliding: Bool { running > 0 }

  /// Creates the glide for `window`.
  ///
  /// - Parameters:
  ///   - window: The window it moves.
  ///   - reduceMotion: Whether Reduce Motion is on; the person's setting (``Appearance/reduceMotion``) unless a test
  ///     says.
  init(window: NSWindow, reduceMotion: @escaping @MainActor () -> Bool = { Appearance.reduceMotion }) {
    self.window = window
    self.reduceMotion = reduceMotion
  }

  /// How long a glide takes: ``Motion/Duration/glide``, or none under Reduce Motion, where the window moves at once.
  ///
  /// - Parameter reduceMotion: Whether Reduce Motion is on.
  /// - Returns: The glide's length, in seconds.
  static func duration(reduceMotion: Bool) -> TimeInterval {
    reduceMotion ? 0 : Motion.Duration.glide.seconds
  }

  /// Glides the window to `frame`, or puts it there at once with Reduce Motion.
  ///
  /// - Parameter frame: Where it lands.
  func move(to frame: NSRect) {
    let duration = Self.duration(reduceMotion: reduceMotion())
    guard duration > 0 else {
      window.setFrame(frame, display: true)
      return
    }
    let curve = Self.curve
    running += 1
    NSAnimationContext.runAnimationGroup { context in
      context.duration = duration
      context.timingFunction = CAMediaTimingFunction(controlPoints: curve.x1, curve.y1, curve.x2, curve.y2)
      window
        .animator()
        .setFrame(frame, display: true)
    } completionHandler: { [weak self] in
      // AppKit runs an animation group's completion handler on the main thread, where the group began, so the main
      // actor is already running here; assumeIsolated says so rather than hopping.
      MainActor.assumeIsolated { self?.running -= 1 }
    }
  }
}
