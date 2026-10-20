import AppKit
import AtlasUI
import SwiftUI

/// The viewer's window: dark, resizable, its controls in the title bar. It
/// reads only while on screen, and gives control back whenever it closes,
/// hides or loses focus.
@MainActor
final class ViewerWindow: NSObject, NSWindowDelegate {
  /// The name its frame is saved under. Shipped, so it never changes.
  static let autosaveName = "AtlasViewer"

  /// The window's controller.
  let controller: ViewerController
  /// The window.
  let window: NSWindow
  /// Runs once the window has closed.
  let closed: @MainActor (ViewerWindow) -> Void
  /// Open until closed, minimised included.
  private(set) var isOpen = false

  /// Creates the window, hidden until ``show()``.
  ///
  /// - Parameters:
  ///   - controller: The window's controller.
  ///   - closed: Runs once the window has closed.
  init(controller: ViewerController, closed: @escaping @MainActor (ViewerWindow) -> Void) {
    self.controller = controller
    self.closed = closed
    // 1200 by 800 the first time, until the person moves or resizes it; at least 760 by 520, where the stage, the
    // footer's controls and the header still fit side by side.
    window = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: 1200, height: 800),
      styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
      backing: .buffered,
      defer: false
    )
    super.init()
    window.title = "Atlas Viewer"
    window.titleVisibility = .hidden
    window.titlebarAppearsTransparent = true
    window.titlebarSeparatorStyle = .none
    // An empty toolbar gives the title bar the header's height, so the
    // window buttons line up with it.
    window.toolbar = NSToolbar(identifier: "viewer")
    window.toolbarStyle = .unified
    window.isReleasedWhenClosed = false
    window.appearance = NSAppearance(named: .darkAqua)
    window.backgroundColor = .ground
    window.contentMinSize = NSSize(width: 760, height: 520)
    window.collectionBehavior.insert(.fullScreenPrimary)
    window.delegate = self
    let host = NSHostingView(
      rootView: ShellRoot {
        ViewerView(state: controller.state, send: controller.send, detail: { controller.headerDetail }) {
          DesktopInput(controller: controller)
        }
      }
    )
    // The window keeps the size it is given; the content fills it.
    host.sizingOptions = []
    window.contentView = host
    window.setFrameAutosaveName(Self.autosaveName)
  }

  /// Whether the window takes the keyboard now.
  var isKey: Bool { window.isKeyWindow }

  /// Brings the window forward, where it was left, and reads while it is on screen. The app shows its Dock icon
  /// first (``DockIcon``).
  func show() {
    if !isOpen && !window.setFrameUsingName(Self.autosaveName) { window.center() }
    isOpen = true
    if window.isMiniaturized { window.deminiaturize(nil) }
    window.makeKeyAndOrderFront(nil)
    NSApp.activate()
    if window.isKeyWindow { controller.gainedFocus() } else { controller.lostFocus() }
    controller.setVisible(true)
  }

  /// Full screen hides the window buttons the header keeps clear of.
  ///
  /// - Parameter notification: The window's notification.
  func windowWillEnterFullScreen(_ notification: Notification) { controller.setFullScreen(true) }

  /// Out of full screen, the window buttons are back.
  ///
  /// - Parameter notification: The window's notification.
  func windowDidExitFullScreen(_ notification: Notification) { controller.setFullScreen(false) }

  /// Closes the window.
  func close() { window.close() }

  /// Reads only while the window is on screen: off screen (minimised, covered, or on another Space) it stops reading
  /// and gives control back; on screen again, it reads again.
  ///
  /// - Parameter notification: The window's notification.
  func windowDidChangeOcclusionState(_ notification: Notification) {
    controller.setVisible(window.occlusionState.contains(.visible) && window.isVisible)
  }

  /// The window takes the keyboard.
  ///
  /// - Parameter notification: The window's notification.
  func windowDidBecomeKey(_ notification: Notification) {
    controller.gainedFocus()
  }

  /// The window lost the keyboard: control goes back.
  ///
  /// - Parameter notification: The window's notification.
  func windowDidResignKey(_ notification: Notification) {
    controller.lostFocus()
  }

  /// The window is closing: control goes back and reading stops; the app settles its Dock icon once `closed` runs.
  ///
  /// - Parameter notification: The window's notification.
  func windowWillClose(_ notification: Notification) {
    isOpen = false
    controller.close()
    Task { closed(self) }
  }
}
