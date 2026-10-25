import AppKit
import AtlasUI
import SwiftUI

/// The alert window: one alert in the widget's look, centred on the screen, for the helper's commands the menu bar
/// panel asks for while no main window is open. Borderless and clear, so the alert's own corners and shadow show
/// (``WindowShell/hostFitting(in:content:)``); it can become key, so Return and Escape press its buttons, and it can
/// be dragged by its card.
final class AlertWindow: NSWindow, WindowPresenting {
  /// The window's controller.
  let controller: AlertController
  /// Open from its presenting until it is dismissed: the alert closes only through its controller.
  private(set) var isPresented = false

  /// Creates the window, hidden until ``present()``.
  ///
  /// - Parameter controller: The window's controller.
  init(controller: AlertController) {
    self.controller = controller
    super.init(contentRect: .zero, styleMask: [.borderless], backing: .buffered, defer: false)
    // Above Atlas's own windows while Atlas is in front, as an alert is, until it is answered; it steps aside with
    // Atlas when another app comes forward, so it never floats over that app, and comes back with Atlas.
    level = .floating
    hidesOnDeactivate = true
    // Borderless, so its card is where it is dragged from.
    isMovableByWindowBackground = true
    collectionBehavior = [.moveToActiveSpace, .fullScreenAuxiliary]
    isReleasedWhenClosed = false
    title = MenuWords.app
    WindowShell.hostFitting(in: self) {
      AlertView(state: controller.state, send: controller.send)
    }
  }

  /// A borderless window takes the keyboard only when it says it can: the alert's buttons answer Return and Escape.
  override var canBecomeKey: Bool { true }

  /// Brings the window forward in the middle of the screen, taking the keyboard.
  func present() {
    if let content = contentViewController?.view {
      setContentSize(content.fittingSize)
    }
    center()
    isPresented = true
    makeKeyAndOrderFront(nil)
    NSApp.activate()
  }

  /// Closes the window.
  func dismiss() {
    isPresented = false
    close()
  }
}
