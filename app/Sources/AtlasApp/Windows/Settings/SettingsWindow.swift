import AppKit
import AtlasUI
import SwiftUI

/// The Settings window: dark, a fixed 680 by 540, its icon tabs on the window's ground with no toolbar behind them.
/// One at a time; it has a Dock icon while open, like any app window (``DockIcon``).
final class SettingsWindow: NSWindow, NSWindowDelegate, WindowPresenting {
  /// The window's controller.
  let controller: SettingsController
  /// Runs once the window has closed and its controller has stopped.
  let closed: @MainActor (SettingsWindow) -> Void
  /// Open until closed, minimised included.
  private(set) var isPresented = false

  /// Creates the window, hidden until ``present()``.
  ///
  /// - Parameters:
  ///   - controller: The window's controller.
  ///   - closed: Runs once the window has closed and its controller has stopped.
  init(controller: SettingsController, closed: @escaping @MainActor (SettingsWindow) -> Void) {
    self.controller = controller
    self.closed = closed
    super.init(
      contentRect: NSRect(origin: .zero, size: SettingsLayout.window),
      styleMask: [.titled, .closable, .miniaturizable, .fullSizeContentView],
      backing: .buffered,
      defer: false
    )
    // The title is for the Window menu alone.
    title = SettingsWords.windowTitle
    titleVisibility = .hidden
    titlebarAppearsTransparent = true
    titlebarSeparatorStyle = .none
    // An empty compact toolbar lowers the window buttons to the board's 14 from the top, beside the tabs.
    toolbar = NSToolbar(identifier: "settings")
    toolbarStyle = .unifiedCompact
    isReleasedWhenClosed = false
    isMovableByWindowBackground = true
    delegate = self
    WindowShell.host(in: self) {
      SettingsView(state: controller.state, edit: controller.update, send: controller.send)
    }
  }

  /// Brings the window forward, centred the first time. The app shows its Dock icon first.
  func present() {
    if !isPresented { center() }
    isPresented = true
    if isMiniaturized { deminiaturize(nil) }
    makeKeyAndOrderFront(nil)
    NSApp.activate()
  }

  /// Closes the window.
  func dismiss() { close() }

  /// The window came forward: what it shows from the other windows is read again.
  ///
  /// - Parameter notification: The window's notification.
  func windowDidBecomeKey(_ notification: Notification) {
    controller.cameForward()
  }

  /// The window is closing: its controller stops, and the app settles its Dock icon once `closed` has run.
  ///
  /// - Parameter notification: The window's notification.
  func windowWillClose(_ notification: Notification) {
    isPresented = false
    controller.close()
    closed(self)
  }
}
