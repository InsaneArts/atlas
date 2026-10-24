import AppKit
import AtlasUI
import SwiftUI

/// The Settings window: dark, 680 wide and as tall as its pane (v3 G2), its symbol tabs on the window's ground with no
/// toolbar behind them. One at a time; it has a Dock icon while open, like any app window (``DockIcon``).
///
/// SwiftUI only reports how tall each pane draws; the window fits it here, in AppKit, keeping its top edge: easing
/// over 0.35 seconds on (0.25, 0.1, 0.25, 1) while on screen, or at once while hidden or under Reduce Motion. A pane
/// changed mid-resize eases on from wherever the window is.
final class SettingsWindow: NSWindow, NSWindowDelegate, WindowPresenting {
  /// Moves a window to a frame, easing or at once.
  typealias Mover = @MainActor (_ window: NSWindow, _ frame: NSRect, _ animated: Bool) -> Void

  /// The window's controller.
  let controller: SettingsController
  /// Runs once the window has closed and its controller has stopped.
  let closed: @MainActor (SettingsWindow) -> Void
  /// Moves the window to each new frame; ``move(_:to:animated:)`` outside tests.
  private let mover: Mover
  /// Open until closed, minimised included.
  private(set) var isPresented = false

  /// Creates the window, hidden until ``present()``.
  ///
  /// - Parameters:
  ///   - controller: The window's controller.
  ///   - mover: Moves the window to each new frame; easing through AppKit's animator by default.
  ///   - closed: Runs once the window has closed and its controller has stopped.
  init(
    controller: SettingsController,
    mover: @escaping Mover = SettingsWindow.move,
    closed: @escaping @MainActor (SettingsWindow) -> Void
  ) {
    self.controller = controller
    self.closed = closed
    self.mover = mover
    super.init(
      contentRect: NSRect(x: 0, y: 0, width: SettingsLayout.width, height: SettingsLayout.firstHeight),
      styleMask: [.titled, .closable, .miniaturizable, .fullSizeContentView],
      backing: .buffered,
      defer: false
    )
    // The title names the pane for the Window menu and VoiceOver; the view draws it in the title bar.
    title = SettingsWords.tab(controller.session.tab)
    titleVisibility = .hidden
    titlebarAppearsTransparent = true
    titlebarSeparatorStyle = .none
    // An empty compact toolbar lowers the window buttons to 14 from the top, beside the pane's name.
    toolbar = NSToolbar(identifier: "settings")
    toolbarStyle = .unifiedCompact
    isReleasedWhenClosed = false
    isMovableByWindowBackground = true
    delegate = self
    WindowShell.host(in: self) {
      SettingsView(state: controller.state, edit: controller.update, send: controller.send)
    }
    controller.reshape = { [weak self] title, height in
      self?.reshape(title: title, contentHeight: height)
    }
  }

  /// Brings the window forward, centred the first time at its pane's height. The app shows its Dock icon first.
  func present() {
    if !isPresented {
      // Draws the pane once, so its height is known before the window shows.
      contentView?.layoutSubtreeIfNeeded()
      controller.reshapeWindow()
      center()
    }
    isPresented = true
    if isMiniaturized { deminiaturize(nil) }
    makeKeyAndOrderFront(nil)
    NSApp.activate()
  }

  /// Closes the window.
  func dismiss() { close() }

  /// Names the pane shown and fits its height, when known.
  ///
  /// - Parameters:
  ///   - title: The pane's name.
  ///   - contentHeight: The title and tabs over the pane, or nil until the pane has drawn.
  func reshape(title: String, contentHeight: CGFloat?) {
    self.title = title
    guard let contentHeight else { return }
    fit(contentHeight: contentHeight)
  }

  /// Fits a content height from the window's frame now, mid-resize included, keeping its top edge.
  ///
  /// - Parameter contentHeight: The title and tabs over the pane.
  func fit(contentHeight: CGFloat) {
    let target = SettingsWindowFrame.frame(current: frame, contentHeight: contentHeight, within: screen?.visibleFrame)
    guard target != frame else { return }
    let reduceMotion = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
    mover(self, target, SettingsWindowFrame.animates(isVisible: isVisible, reduceMotion: reduceMotion))
  }

  /// Moves a window to a frame: through AppKit's animator over ``AtlasUI/Motion/Duration/resize`` on
  /// ``AtlasUI/Motion/resizeCurve``, which starts from wherever the window is, so a new frame mid-resize retargets;
  /// or at once.
  ///
  /// - Parameters:
  ///   - window: The window.
  ///   - frame: Its new frame.
  ///   - animated: Whether it eases there.
  static func move(_ window: NSWindow, to frame: NSRect, animated: Bool) {
    guard animated else {
      window.setFrame(frame, display: true)
      return
    }
    let curve = Motion.resizeCurve
    NSAnimationContext.runAnimationGroup { context in
      context.duration = Motion.Duration.resize.seconds
      context.timingFunction = CAMediaTimingFunction(controlPoints: curve.x1, curve.y1, curve.x2, curve.y2)
      window.animator()
        .setFrame(frame, display: true)
    }
  }

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
    controller.reshape = nil
    controller.close()
    closed(self)
  }
}
