import AppKit
import AtlasKit
import AtlasUI
import SwiftUI

/// Atlas's menu bar item and its panel. The glyph follows the helper's mode; the panel reads the desktop only while
/// it is open.
@MainActor
final class PanelPopover: NSObject, NSPopoverDelegate {
  /// The panel's controller.
  let controller: PanelController
  /// The menu bar item.
  private let item = NSStatusBar.system
    .statusItem(withLength: NSStatusItem.variableLength)
  /// The panel.
  private let popover = NSPopover()

  /// Puts the item in the menu bar and starts reading at the closed rate.
  ///
  /// - Parameter controller: The panel's controller.
  init(controller: PanelController) {
    self.controller = controller
    super.init()
    let content = NSHostingController(
      rootView: ShellRoot {
        PanelView(state: controller.state, send: controller.send)
      }
    )
    // The popover follows the panel as rows and messages come and go.
    content.sizingOptions = [.preferredContentSize]
    popover.contentViewController = content
    popover.behavior = .transient
    // Atlas's panel is always dark, like its other windows.
    popover.appearance = NSAppearance(named: .darkAqua)
    popover.delegate = self
    item.button?.target = self
    item.button?.action = #selector(toggle)
    StateTracking.follow(
      { [weak controller] in controller?.mode },
      apply: { [weak self] mode in
        self?.item.button?.image = StatusGlyph.image(for: mode)
        self?.item.button?.toolTip = PanelWords.statusItem(mode.label)
      }
    )
    controller.watch(open: false)
    Task { await controller.findHelper() }
  }

  /// Opens the panel, or closes it when open.
  @objc private func toggle() {
    guard let button = item.button else { return }
    if popover.isShown {
      popover.performClose(nil)
    } else {
      popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
      // No row is focused until the person presses Tab, as in menus.
      let window = popover.contentViewController?.view.window
      window?.makeFirstResponder(nil)
      Task { window?.makeFirstResponder(nil) }
      controller.watch(open: true)
    }
  }

  /// Closes the panel, as Open Viewer does.
  func closePanel() {
    if popover.isShown { popover.performClose(nil) }
  }

  /// The panel closed: reading slows to the closed rate.
  ///
  /// - Parameter notification: The popover's notification.
  func popoverDidClose(_ notification: Notification) {
    controller.watch(open: false)
  }
}
