import AppKit
import AtlasUI

extension GlideMotion {
  /// The animations as AppKit runs them.
  static let appKit = GlideMotion(
    fadeContent: GlideMotion.fadeContent(_:to:over:done:),
    fadeWindow: GlideMotion.fadeWindow(_:to:over:done:),
    travel: GlideMotion.travel(_:to:over:done:),
    reveal: GlideMotion.reveal(_:after:over:)
  )

  /// Fades the window's content to `alpha` over `duration`, then calls `done`; a window with no content calls it at
  /// once.
  ///
  /// - Parameters:
  ///   - window: The window.
  ///   - alpha: The opacity it fades to.
  ///   - duration: How long the fade takes.
  ///   - done: Called once it has faded.
  private static func fadeContent(
    _ window: NSWindow,
    to alpha: CGFloat,
    over duration: TimeInterval,
    done: @escaping @MainActor () -> Void
  ) {
    guard let content = window.contentView else { return done() }
    // A reveal still under way gives way to this fade.
    content.animations = [:]
    fade(over: duration, done: done) {
      content
        .animator()
        .alphaValue = alpha
    }
  }

  /// Fades the whole window to `alpha` over `duration`, then calls `done`.
  ///
  /// - Parameters:
  ///   - window: The window.
  ///   - alpha: The opacity it fades to.
  ///   - duration: How long the fade takes.
  ///   - done: Called once it has faded.
  private static func fadeWindow(
    _ window: NSWindow,
    to alpha: CGFloat,
    over duration: TimeInterval,
    done: @escaping @MainActor () -> Void
  ) {
    fade(over: duration, done: done) {
      window
        .animator()
        .alphaValue = alpha
    }
  }

  /// Glides the window to `frame` over `duration` on ``Motion/glideSoftCurve``, or sets it there at once when
  /// `duration` is 0, then calls `done`.
  ///
  /// - Parameters:
  ///   - window: The window.
  ///   - frame: Where it lands.
  ///   - duration: How long it travels.
  ///   - done: Called once it has landed.
  private static func travel(
    _ window: NSWindow,
    to frame: NSRect,
    over duration: TimeInterval,
    done: @escaping @MainActor () -> Void
  ) {
    guard duration > 0 else {
      window.setFrame(frame, display: true)
      return done()
    }
    let curve = Motion.glideSoftCurve
    NSAnimationContext.runAnimationGroup { context in
      context.duration = duration
      context.timingFunction = CAMediaTimingFunction(controlPoints: curve.x1, curve.y1, curve.x2, curve.y2)
      window
        .animator()
        .setFrame(frame, display: true)
    } completionHandler: {
      // AppKit runs an animation group's completion handler on the main thread, where the group began, so the main
      // actor is already running here; assumeIsolated says so rather than hopping.
      MainActor.assumeIsolated { done() }
    }
  }

  /// Fades the content back in over `duration`, holding it hidden for `delay` first, all in one animation, so nothing
  /// waits on a timer.
  ///
  /// - Parameters:
  ///   - window: The window.
  ///   - delay: How long the content stays hidden first.
  ///   - duration: How long it fades in.
  private static func reveal(_ window: NSWindow, after delay: TimeInterval, over duration: TimeInterval) {
    guard let content = window.contentView else { return }
    let total = delay + duration
    let hold = CAKeyframeAnimation()
    hold.values = [0, 0, 1]
    hold.keyTimes = [0, NSNumber(value: delay / total), 1]
    content.animations = [alphaKey: hold]
    NSAnimationContext.runAnimationGroup { context in
      context.duration = total
      content
        .animator()
        .alphaValue = 1
    } completionHandler: {
      MainActor.assumeIsolated { content.animations = [:] }
    }
  }

  /// Runs `change`, an opacity change through an animator, over `duration`, then runs `done`.
  ///
  /// - Parameters:
  ///   - duration: How long the fade takes.
  ///   - done: Runs once it has faded.
  ///   - change: The change, such as a window's opacity through its animator.
  private static func fade(
    over duration: TimeInterval,
    done: @escaping @MainActor () -> Void,
    _ change: () -> Void
  ) {
    NSAnimationContext.runAnimationGroup { context in
      context.duration = duration
      change()
    } completionHandler: {
      MainActor.assumeIsolated { done() }
    }
  }

  /// The animations key AppKit looks up for an opacity change.
  private static let alphaKey = "alphaValue"
}
