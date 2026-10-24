import AppKit
import AtlasUI

/// Moves the setup window to a new place on the screen in ``GlideSteps``: its content fades out, the window glides on
/// ``Motion/glideSoftCurve``, starting at once and landing softly, and the content fades back in near the end (v3 S7,
/// X13); under Reduce Motion the window fades out, lands and fades in. It counts the moves running, so nothing else
/// moves the window meanwhile.
final class WindowGlide {
  /// Carries out one move: the window, where it lands, the steps, and what to call once it has landed and shows again.
  typealias Animate = @MainActor (NSWindow, NSRect, GlideSteps, @escaping @MainActor () -> Void) -> Void

  /// The window it moves, which owns this glide and so outlives it.
  private unowned let window: NSWindow
  /// Whether Reduce Motion is on, read as each move starts.
  private let reduceMotion: @MainActor () -> Bool
  /// Carries out a move.
  private let animate: Animate
  /// Moves running now.
  private var running = 0
  /// Whether a move runs now.
  var isGliding: Bool { running > 0 }

  /// Creates the glide for `window`.
  ///
  /// - Parameters:
  ///   - window: The window it moves.
  ///   - reduceMotion: Whether Reduce Motion is on; the person's setting (``Appearance/reduceMotion``) unless a test
  ///     says.
  ///   - animate: Carries out a move; AppKit's animations unless a test records them.
  init(
    window: NSWindow,
    reduceMotion: @escaping @MainActor () -> Bool = { Appearance.reduceMotion },
    animate: @escaping Animate = WindowGlide.run
  ) {
    self.window = window
    self.reduceMotion = reduceMotion
    self.animate = animate
  }

  /// Moves the window to `frame`: a glide, its content fading around it when the window changes size, or, under Reduce
  /// Motion, a fade out and in.
  ///
  /// - Parameter frame: Where it lands.
  func move(to frame: NSRect) {
    let steps = GlideSteps(reduceMotion: reduceMotion(), resizes: frame.size != window.frame.size)
    running += 1
    animate(window, frame, steps) { [weak self] in self?.running -= 1 }
  }

  /// Carries out `steps` with AppKit's animations: the fade out, then the move, the fade in starting part way through
  /// it.
  ///
  /// - Parameters:
  ///   - window: The window.
  ///   - frame: Where it lands.
  ///   - steps: The move's steps.
  ///   - landed: Called once the window has landed and shows again.
  static func run(_ window: NSWindow, to frame: NSRect, _ steps: GlideSteps, landed: @escaping @MainActor () -> Void) {
    switch steps.fading {
    case .none:
      travel(window, to: frame, steps, landed: landed)
    case .content:
      guard let content = window.contentView else { return travel(window, to: frame, steps, landed: landed) }
      // A reveal still under way from the last move gives way to this fade out.
      content.animations = [:]
      fade(over: steps.fadeOut) {
        content
          .animator()
          .alphaValue = 0
      } then: {
        travel(window, to: frame, steps, landed: landed)
        reveal(content, after: steps.fadeInDelay, over: steps.fadeIn)
      }
    case .window:
      fade(over: steps.fadeOut) {
        window
          .animator()
          .alphaValue = 0
      } then: {
        window.setFrame(frame, display: true)
        fade(over: steps.fadeIn) {
          window
            .animator()
            .alphaValue = 1
        } then: {
          landed()
        }
      }
    }
  }

  /// Glides the window to `frame` over the steps' travel.
  ///
  /// - Parameters:
  ///   - window: The window.
  ///   - frame: Where it lands.
  ///   - steps: The move's steps.
  ///   - landed: Called once it has landed.
  private static func travel(
    _ window: NSWindow,
    to frame: NSRect,
    _ steps: GlideSteps,
    landed: @escaping @MainActor () -> Void
  ) {
    let curve = Motion.glideSoftCurve
    NSAnimationContext.runAnimationGroup { context in
      context.duration = steps.travel
      context.timingFunction = CAMediaTimingFunction(controlPoints: curve.x1, curve.y1, curve.x2, curve.y2)
      window
        .animator()
        .setFrame(frame, display: true)
    } completionHandler: {
      // AppKit runs an animation group's completion handler on the main thread, where the group began, so the main
      // actor is already running here; assumeIsolated says so rather than hopping.
      MainActor.assumeIsolated { landed() }
    }
  }

  /// Fades the content back in over `duration`, holding it hidden for `delay` first, all in one animation, so nothing
  /// waits on a timer.
  ///
  /// - Parameters:
  ///   - content: The window's content.
  ///   - delay: How long it stays hidden first.
  ///   - duration: How long it fades in.
  private static func reveal(_ content: NSView, after delay: TimeInterval, over duration: TimeInterval) {
    let total = delay + duration
    let hold = CAKeyframeAnimation()
    hold.values = [0, 0, 1]
    hold.keyTimes = [0, NSNumber(value: delay / total), 1]
    content.animations = [Self.alphaKey: hold]
    NSAnimationContext.runAnimationGroup { context in
      context.duration = total
      content
        .animator()
        .alphaValue = 1
    } completionHandler: {
      MainActor.assumeIsolated { content.animations = [:] }
    }
  }

  /// Runs `change`, an opacity change through an animator, over `duration`, then runs `then`.
  ///
  /// - Parameters:
  ///   - duration: How long the fade takes.
  ///   - change: The change, such as a window's opacity through its animator.
  ///   - then: Runs once it has faded.
  private static func fade(
    over duration: TimeInterval,
    _ change: () -> Void,
    then: @escaping @MainActor () -> Void
  ) {
    NSAnimationContext.runAnimationGroup { context in
      context.duration = duration
      change()
    } completionHandler: {
      MainActor.assumeIsolated { then() }
    }
  }

  /// The animations key AppKit looks up for an opacity change.
  private static let alphaKey = "alphaValue"
}
