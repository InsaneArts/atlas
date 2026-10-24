import AppKit

/// The animations a ``WindowGlide`` builds each move from, each calling back once it has run: AppKit's
/// (``appKit``), or a test's, which holds each until the test ends it. The glide orders them, so a move that a newer
/// one supersedes stops at its next step.
struct GlideMotion {
  /// Carries out one animation on the window, over a time in seconds, then calls back.
  typealias Animation<Value> = @MainActor (NSWindow, Value, TimeInterval, @escaping @MainActor () -> Void) -> Void

  /// Fades the window's content to an opacity, then calls back.
  let fadeContent: Animation<CGFloat>
  /// Fades the whole window to an opacity, then calls back.
  let fadeWindow: Animation<CGFloat>
  /// Moves the window to a frame on ``Motion/glideSoftCurve``, at once over no time, then calls back.
  let travel: Animation<NSRect>
  /// Brings the content back in over a time, holding it hidden for the time given first.
  let reveal: @MainActor (NSWindow, _ delay: TimeInterval, _ duration: TimeInterval) -> Void
}
