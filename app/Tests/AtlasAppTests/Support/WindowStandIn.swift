@testable import AtlasApp

/// A window that is only a flag: what the coordinator opens in a test, which reaches no screen. Closing it calls its
/// close hook at once, as Settings' does, or, with ``closesLater``, only at ``finishClosing()``, as the main window's
/// and the setup window's do once AppKit, or their controller, has finished.
final class WindowStandIn: WindowPresenting {
  /// Whether the coordinator has it open.
  private(set) var isPresented = false
  /// Whether it takes the keyboard; a test sets it.
  var isKeyWindow = false
  /// How often it was presented: opened, or brought forward.
  private(set) var presentations = 0
  /// Whether closing waits for ``finishClosing()`` to call the close hook; a test sets it.
  var closesLater = false
  /// Whether it closed and has not called its close hook yet.
  private var closing = false
  /// The coordinator's close hook.
  private let closed: @MainActor (any WindowPresenting) -> Void

  /// A closed stand-in.
  ///
  /// - Parameter closed: The coordinator's close hook.
  init(closed: @escaping @MainActor (any WindowPresenting) -> Void) {
    self.closed = closed
  }

  /// Marks the window open, counting the presentation.
  func present() {
    presentations += 1
    isPresented = true
  }

  /// Marks the window closed and calls the close hook, now or, with ``closesLater``, at ``finishClosing()``; a closed
  /// window does nothing.
  func dismiss() {
    guard isPresented else { return }
    isPresented = false
    if closesLater {
      closing = true
    } else {
      closed(self)
    }
  }

  /// Calls the close hook a closing held back.
  func finishClosing() {
    guard closing else { return }
    closing = false
    closed(self)
  }
}
