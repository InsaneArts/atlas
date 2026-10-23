import AppKit
import AtlasUI

/// Moves the setup window to a new place on the screen: a glide that leaves softly, travels quickly and lands softly
/// over ``Motion/Duration/glide``, or at once with Reduce Motion. It counts the glides running, so nothing else moves
/// the window meanwhile.
@MainActor
final class WindowGlide {
  /// The glide's curve, its control points: an ease in and out steeper than AppKit's own.
  static let curve = (x1: Float(0.65), y1: Float(0), x2: Float(0.35), y2: Float(1))

  /// The window it moves.
  private let window: NSWindow
  /// Glides running now.
  private var running = 0
  /// Whether a glide runs now.
  var isGliding: Bool { running > 0 }

  /// Creates the glide for `window`.
  ///
  /// - Parameter window: The window it moves.
  init(window: NSWindow) {
    self.window = window
  }

  /// Glides the window to `frame`, or puts it there at once with Reduce Motion.
  ///
  /// - Parameter frame: Where it lands.
  func move(to frame: NSRect) {
    guard !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion else {
      window.setFrame(frame, display: true)
      return
    }
    let curve = Self.curve
    running += 1
    NSAnimationContext.runAnimationGroup { context in
      context.duration = Motion.Duration.glide.seconds
      context.timingFunction = CAMediaTimingFunction(controlPoints: curve.x1, curve.y1, curve.x2, curve.y2)
      window
        .animator()
        .setFrame(frame, display: true)
    } completionHandler: { [weak self] in
      MainActor.assumeIsolated { self?.running -= 1 }
    }
  }
}
