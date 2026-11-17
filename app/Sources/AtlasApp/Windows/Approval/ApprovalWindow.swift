import AppKit
import AtlasUI
import SwiftUI

/// The approval window: what a command asks the person to allow, in the alert's look, centred on the screen above
/// other windows. Borderless and clear, so the alert's own corners and shadow show
/// (``WindowShell/hostFitting(in:content:)``); it can become key, so Return and Escape press its buttons, and it can
/// be dragged by its card. It is the app's only window, so it stays up when another app comes forward. The native
/// Close, ⌘W and File › Close among them, goes through its controller, so closing it refuses a request still asked.
final class ApprovalWindow: NSWindow {
  /// The window's controller.
  let controller: ApprovalController

  /// Creates the window, hidden until ``present()``.
  ///
  /// - Parameter controller: The window's controller.
  init(controller: ApprovalController) {
    self.controller = controller
    super.init(contentRect: .zero, styleMask: [.borderless], backing: .buffered, defer: false)
    level = .floating
    isMovableByWindowBackground = true
    collectionBehavior = [.moveToActiveSpace, .fullScreenAuxiliary]
    isReleasedWhenClosed = false
    title = MenuWords.app
    WindowShell.hostFitting(in: self) {
      ApprovalView(state: controller.state, send: controller.send)
    }
  }

  /// A borderless window takes the keyboard only when it says it can: the alert's buttons answer Return and Escape.
  override var canBecomeKey: Bool { true }

  /// The native Close, which a borderless window would otherwise refuse with a beep: it closes as Escape and Close
  /// do, through the controller, refusing first while the person may still answer.
  ///
  /// - Parameter sender: What asked, unused.
  override func performClose(_ sender: Any?) {
    controller.send(.close)
  }

  /// Brings the window forward in the middle of the screen, taking the keyboard.
  func present() {
    if let content = contentViewController?.view {
      setContentSize(content.fittingSize)
    }
    center()
    makeKeyAndOrderFront(nil)
    NSApp.activate()
  }
}
