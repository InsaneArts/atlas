import AppKit
import AtlasUI
import SwiftUI

/// The alert window: one alert in the widget's look, centred on the screen, for the helper's commands the menu bar
/// panel asks for while no main window is open. Borderless and clear, so the alert's own corners and shadow show; it
/// can become key, so Return and Escape press its buttons.
@MainActor
final class AlertWindow: NSWindow {
  /// The window's controller.
  let controller: AlertController

  /// Creates the window, hidden until ``show()``.
  ///
  /// - Parameter controller: The window's controller.
  init(controller: AlertController) {
    self.controller = controller
    super.init(contentRect: .zero, styleMask: [.borderless], backing: .buffered, defer: false)
    isOpaque = false
    backgroundColor = .clear
    // The alert draws its own shadow inside the window's clear frame.
    hasShadow = false
    // Above other windows, as an alert is, until it is answered.
    level = .floating
    collectionBehavior = [.moveToActiveSpace, .fullScreenAuxiliary]
    isReleasedWhenClosed = false
    appearance = NSAppearance(named: .darkAqua)
    title = MenuWords.app
    let content = NSHostingController(
      rootView: ShellRoot {
        AlertView(state: controller.state, send: controller.send)
      }
    )
    // The window follows the alert as its words change.
    content.sizingOptions = [.preferredContentSize]
    contentViewController = content
  }

  /// A borderless window takes the keyboard only when it says it can: the alert's buttons answer Return and Escape.
  override var canBecomeKey: Bool { true }

  /// Brings the window forward in the middle of the screen, taking the keyboard.
  func show() {
    if let content = contentViewController?.view {
      setContentSize(content.fittingSize)
    }
    center()
    makeKeyAndOrderFront(nil)
    NSApp.activate()
  }
}
