import AppKit
import AtlasUI
import SwiftUI

/// The Settings window: dark, a fixed 680 by 540, its icon tabs on the window's ground with no toolbar behind them.
/// One at a time; it has a Dock icon while open, like any app window (``DockIcon``).
@MainActor
final class SettingsWindow: NSObject, NSWindowDelegate {
  /// The window's controller.
  let controller: SettingsController
  /// The window.
  let window: NSWindow
  /// Runs once the window has closed and its controller has stopped.
  let closed: @MainActor (SettingsWindow) -> Void
  /// Open until closed, minimised included.
  private(set) var isOpen = false

  /// Creates the window, hidden until ``show()``.
  ///
  /// - Parameters:
  ///   - controller: The window's controller.
  ///   - closed: Runs once the window has closed and its controller has stopped.
  init(controller: SettingsController, closed: @escaping @MainActor (SettingsWindow) -> Void) {
    self.controller = controller
    self.closed = closed
    window = NSWindow(
      contentRect: NSRect(origin: .zero, size: SettingsLayout.window),
      styleMask: [.titled, .closable, .miniaturizable, .fullSizeContentView],
      backing: .buffered,
      defer: false
    )
    super.init()
    // The title is for the Window menu alone.
    window.title = SettingsWords.windowTitle
    window.titleVisibility = .hidden
    window.titlebarAppearsTransparent = true
    window.titlebarSeparatorStyle = .none
    // An empty compact toolbar lowers the window buttons to the board's 14 from the top, beside the tabs.
    window.toolbar = NSToolbar(identifier: "settings")
    window.toolbarStyle = .unifiedCompact
    window.isReleasedWhenClosed = false
    window.isMovableByWindowBackground = true
    window.appearance = NSAppearance(named: .darkAqua)
    window.backgroundColor = .ground
    window.delegate = self
    let host = NSHostingView(
      rootView: ShellRoot {
        SettingsView(state: controller.state, edit: controller.update, send: controller.send)
      }
    )
    // The window keeps its one size; the content fills it.
    host.sizingOptions = []
    window.contentView = host
  }

  /// Whether the window takes the keyboard now.
  var isKey: Bool { window.isKeyWindow }

  /// Brings the window forward, centred the first time. The app shows its Dock icon first.
  func show() {
    if !isOpen { window.center() }
    isOpen = true
    if window.isMiniaturized { window.deminiaturize(nil) }
    window.makeKeyAndOrderFront(nil)
    NSApp.activate()
  }

  /// Closes the window.
  func close() { window.close() }

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
    isOpen = false
    controller.close()
    closed(self)
  }
}
