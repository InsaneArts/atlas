import AppKit
import AtlasUI
import SwiftUI

/// The Settings window: 740 by 572, as Rex's Settings stands, with the sidebar of pages under its window buttons and the
/// content card beside it, titled with the route shown for the Window menu and VoiceOver. One at a time; it has a Dock
/// icon while open, like any app window (``DockIcon``). It opens where it was left, kept whole on its screen
/// (``SettingsWindowFrame``), centred only the first time; it cannot be resized or minimised, as a settings window is
/// closed instead, and it comes to the Space the person is on, full-screen apps included. A press anywhere outside the
/// search field takes the keyboard back from it (``FieldFocus``), and the mouse's back and forward buttons move
/// through the window's history as ⌘[ and ⌘] do.
final class SettingsWindow: NSWindow, NSWindowDelegate, WindowPresenting {
  /// The name its place is kept under in the app's defaults. Shipped, so it never changes.
  static let autosaveName = "AtlasSettings"
  /// 3: the mouse's back button, as AppKit numbers it.
  static let backButton = 3
  /// 4: the mouse's forward button.
  static let forwardButton = 4

  /// The window's controller.
  let controller: SettingsController
  /// Runs once the window has closed and its controller has stopped.
  let closed: @MainActor (SettingsWindow) -> Void
  /// Takes the keyboard back from the search field on a press anywhere else in the window.
  let fieldFocus = FieldFocus()
  /// Open until closed.
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
      contentRect: NSRect(x: 0, y: 0, width: SettingsLayout.width, height: SettingsLayout.height),
      styleMask: [.titled, .closable, .fullSizeContentView],
      backing: .buffered,
      defer: false
    )
    // The title names the route for the Window menu and VoiceOver; the view draws it in its bar.
    title = controller.state.title
    titleVisibility = .hidden
    titlebarAppearsTransparent = true
    titlebarSeparatorStyle = .none
    // An empty unified toolbar gives the title bar its 52, so the window buttons centre with the sidebar's band and
    // the back and forward capsule, as the main window's do.
    toolbar = NSToolbar(identifier: "settings")
    toolbarStyle = .unified
    isReleasedWhenClosed = false
    isMovableByWindowBackground = true
    collectionBehavior = [.fullScreenAuxiliary, .moveToActiveSpace]
    delegate = self
    let fieldReport = fieldFocus.report
    WindowShell.host(in: self) {
      SettingsView(state: controller.state, edit: controller.update, send: controller.send)
        .environment(\.focusedFieldReport, fieldReport)
    }
    setFrameAutosaveName(Self.autosaveName)
    followTitle()
    followAccounts()
  }

  /// Keeps the window's history to accounts Atlas lists, each time a reading changes the list.
  private func followAccounts() {
    StateTracking.follow { [weak self] () -> Set<String>?? in
      guard let self else { return nil }
      return .some(controller.listedAccounts)
    } apply: { [weak self] (_: Set<String>?) in
      self?.controller.keepListedAccounts()
    }
  }

  /// Titles the window with the route shown, each time it changes.
  private func followTitle() {
    StateTracking.follow { [weak self] in
      self?.controller.state.title
    } apply: { [weak self] title in
      if self?.title != title { self?.title = title }
    }
  }

  /// Brings the window forward where it was left, kept whole on its screen, or centred the first time. The app shows
  /// its Dock icon first.
  func present() {
    if !isPresented {
      let restored = setFrameUsingName(Self.autosaveName)
      if restored { keepOnScreen() } else { center() }
    }
    isPresented = true
    fieldFocus.watch(self)
    makeKeyAndOrderFront(nil)
    NSApp.activate()
  }

  /// Closes the window.
  func dismiss() { close() }

  /// Sets the window at its fixed size, moved just enough to stand whole on its screen.
  func keepOnScreen() {
    let target = SettingsWindowFrame.frame(current: frame, within: screen?.visibleFrame)
    if target != frame { setFrame(target, display: true) }
  }

  /// The mouse's back and forward buttons move through the window's history; any other button goes on as usual.
  ///
  /// - Parameter event: The press.
  override func otherMouseDown(with event: NSEvent) {
    switch event.buttonNumber {
    case Self.backButton: controller.send(.goBack)
    case Self.forwardButton: controller.send(.goForward)
    default: super.otherMouseDown(with: event)
    }
  }

  /// The window came forward: what it shows from the other windows is read again.
  ///
  /// - Parameter notification: The window's notification.
  func windowDidBecomeKey(_ notification: Notification) {
    controller.cameForward()
  }

  /// The window is closing: its controller stops, the search gives up watching presses, and the app settles its Dock
  /// icon once `closed` has run.
  ///
  /// - Parameter notification: The window's notification.
  func windowWillClose(_ notification: Notification) {
    isPresented = false
    fieldFocus.stop()
    controller.close()
    closed(self)
  }
}
