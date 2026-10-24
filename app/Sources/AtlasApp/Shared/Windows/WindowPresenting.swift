import AppKit

/// A window the coordinator opens and closes: each window's AppKit shell, or a test's `WindowStandIn`. The names
/// differ from `NSWindow`'s own `close()` and `isVisible` on purpose: presenting a window is more than ordering it
/// front (it is centred or put back where it was left, and its controller starts reading).
protocol WindowPresenting: AnyObject {
  /// Whether the window is open: from its presenting until it closes, minimised included.
  var isPresented: Bool { get }
  /// Whether the window takes the keyboard now, as `NSWindow` says.
  var isKeyWindow: Bool { get }
  /// Opens the window, or brings it forward when open.
  func present()
  /// Closes the window. Closing a closed window does nothing.
  func dismiss()
}
