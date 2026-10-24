import AppKit
import AtlasUI
import SwiftUI

/// The single resizable Settings window. Its frame belongs to the person, independently of the selected pane.
/// It restores its autosaved frame, keeps native traffic lights and follows the current Space without a minimize control.
final class SettingsWindow: NSWindow, NSWindowDelegate, WindowPresenting {
  /// The shipped autosave identity.
  static let autosaveName = "AtlasSettings"
  /// The window's controller.
  let controller: SettingsController
  /// Runs once the window closes and its controller stops reading.
  let closed: @MainActor (SettingsWindow) -> Void
  /// Open until closed.
  private(set) var isPresented = false

  /// Creates the window, hidden until presentation.
  ///
  /// - Parameters:
  ///   - controller: The window's controller.
  ///   - closed: Runs once the window closes and its controller stops reading.
  init(controller: SettingsController, closed: @escaping @MainActor (SettingsWindow) -> Void) {
    self.controller = controller
    self.closed = closed
    super.init(
      contentRect: NSRect(x: 0, y: 0, width: SettingsLayout.defaultWidth, height: SettingsLayout.defaultHeight),
      styleMask: [.titled, .closable, .resizable, .fullSizeContentView],
      backing: .buffered,
      defer: false
    )
    title = "Settings — \(SettingsWords.tab(controller.session.tab))"
    titleVisibility = .hidden
    titlebarAppearsTransparent = true
    titlebarSeparatorStyle = .none
    toolbar = NSToolbar(identifier: "settings")
    toolbarStyle = .unifiedCompact
    isReleasedWhenClosed = false
    isMovableByWindowBackground = true
    collectionBehavior = [.fullScreenAuxiliary, .moveToActiveSpace]
    delegate = self
    WindowShell.host(in: self) {
      SettingsView(state: controller.state, edit: controller.update, send: controller.send)
    }
    controller.nameWindow = { [weak self] title in self?.title = title }
    setFrameAutosaveName(Self.autosaveName)
    validateFrame()
  }

  /// Restores the person's frame on first presentation, centering only when no saved placement exists.
  func present() {
    if !isPresented {
      let restored = setFrameUsingName(Self.autosaveName)
      if !restored { center() }
      validateFrame()
    }
    isPresented = true
    makeKeyAndOrderFront(nil)
    NSApp.activate()
  }

  /// Closes the window.
  func dismiss() { close() }

  /// Fits restored placement to the usable screen while keeping content independently scrollable.
  func validateFrame() {
    let visible = screen?.visibleFrame ?? NSScreen.main?.visibleFrame
    contentMinSize = SettingsWindowFrame.minimum(within: visible)
    let usable = SettingsWindowFrame.frame(current: frame, within: visible)
    if frame != usable { setFrame(usable, display: false) }
  }

  /// Revalidates the minimum when the window moves to a physically smaller screen.
  ///
  /// - Parameter notification: The window's notification.
  func windowDidChangeScreen(_ notification: Notification) {
    validateFrame()
  }

  /// Refreshes the existing shared facts and agent reader when the window comes forward.
  ///
  /// - Parameter notification: The window's notification.
  func windowDidBecomeKey(_ notification: Notification) {
    controller.cameForward()
  }

  /// Stops the reader and lets the coordinator settle the Dock icon after closing.
  ///
  /// - Parameter notification: The window's notification.
  func windowWillClose(_ notification: Notification) {
    isPresented = false
    controller.nameWindow = nil
    controller.close()
    closed(self)
  }
}
