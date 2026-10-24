import AppKit
import AtlasUI
import SwiftUI

/// The menu bar panel's popover, opened from Atlas's menu bar item (``StatusItem``), whose glyph it keeps on the
/// helper's mode. The panel reads the picture only while it is open, and opens its ⋯ menu here, under its button
/// (`PanelPopover+Menu`). A popover, not a window, so the one shell name that does not end in `Window`.
final class PanelPopover: NSObject, NSPopoverDelegate {
  /// 6: between ⋯ and the menu it opens, as in the main window.
  static let menuGap: CGFloat = 6

  /// The panel's controller.
  let controller: PanelController
  /// Opens the Settings window, from the ⋯ menu.
  let openSettings: @MainActor () -> Void
  /// The menu bar item it opens from.
  private let item: StatusItem
  /// The panel.
  let popover = NSPopover()
  /// Watches for clicks outside Atlas while the panel shows, so a click anywhere else closes it as a menu would;
  /// nil while it is closed. A transient popover only closes on clicks this app receives, and a menu bar app is
  /// often not the active one.
  private var outsideClicks: Any?

  /// Opens from the item in the menu bar and starts reading at the closed rate.
  ///
  /// - Parameters:
  ///   - controller: The panel's controller.
  ///   - item: The menu bar item the panel opens from.
  ///   - openSettings: Opens the Settings window, from the ⋯ menu.
  init(controller: PanelController, item: StatusItem, openSettings: @escaping @MainActor () -> Void) {
    self.controller = controller
    self.item = item
    self.openSettings = openSettings
    super.init()
    // The popover follows the panel as Recent and the line come and go (`PanelPopover+Resize`); Atlas's panel is
    // always dark, like its other windows.
    WindowShell.host(
      in: popover,
      label: PanelWords.name,
      follow: { [weak self] in self?.resize(to: $0) },
      content: { [weak self] in
        PanelView(state: controller.state) { self?.handle($0) }
      }
    )
    popover.behavior = .transient
    popover.delegate = self
    item.button?.target = self
    item.button?.action = #selector(toggle)
    StateTracking.follow(
      { [weak controller] in controller?.mode },
      apply: { [weak self] mode in
        self?.item.show(mode.kind, description: PanelWords.statusItem(mode.word))
      }
    )
    controller.watch(open: false)
  }

  /// What the person did in the panel: ⋯ opens its menu here, everything else goes to the controller.
  ///
  /// - Parameter event: The event.
  func handle(_ event: PanelEvent) {
    switch event {
    case .showMore(let frame): openMore(below: frame)
    default: controller.send(event)
    }
  }

  /// Opens the panel, or closes it when open.
  @objc private func toggle() {
    guard let button = item.button else { return }
    if popover.isShown {
      popover.performClose(nil)
    } else {
      popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
      // The panel takes the keyboard, as a menu does, so Escape and clicks elsewhere close it.
      NSApp.activate()
      let window = popover.contentViewController?.view.window
      window?.makeKey()
      // No button is focused until the person presses Tab, as in menus.
      window?.makeFirstResponder(nil)
      // Again after this turn, once the popover has finished opening. Unstructured, as opening is synchronous.
      Task { window?.makeFirstResponder(nil) }
      watchOutsideClicks()
      controller.watch(open: true)
    }
  }

  /// Closes the panel, as opening a window or an alert from it does.
  func closePanel() {
    if popover.isShown { popover.performClose(nil) }
  }

  /// Opens the ⋯ menu under its button, the button pressed until the menu closes.
  ///
  /// - Parameter frame: The button's frame in the panel, top left first.
  private func openMore(below frame: CGRect) {
    guard let content = popover.contentViewController?.view else { return }
    let menu = PanelMoreMenu.menu(
      controller.moreMenu,
      target: self,
      actions: PanelMoreMenu.Actions(
        openAtlas: { [weak self] in self?.controller.send(.openAtlas) },
        openSettings: { [weak self] in
          self?.closePanel()
          self?.openSettings()
        },
        setOpenAtLogin: { [weak self] on in
          guard let controller = self?.controller else { return }
          // Unstructured, as a menu item's action is synchronous.
          Task { await controller.setOpenAtLogin(on) }
        },
        quit: { [weak self] in self?.controller.links.quit() }
      )
    )
    controller.setMoreOpen(true)
    // SwiftUI's frames run down from the top; a view that is not flipped runs up from the bottom.
    let below = frame.maxY + Self.menuGap
    let point = NSPoint(x: frame.minX, y: content.isFlipped ? below : content.bounds.height - below)
    menu.popUp(positioning: nil, at: point, in: content)
    controller.setMoreOpen(false)
  }

  /// Closes the panel on a click in another app, until the panel closes. Clicks in Atlas's own windows reach the
  /// popover itself, which closes on them as a transient popover does.
  private func watchOutsideClicks() {
    stopWatchingOutsideClicks()
    outsideClicks = NSEvent.addGlobalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown, .otherMouseDown]) {
      [weak self] _ in
      // Unstructured, as a monitor's handler is synchronous: the panel closes after it, on the main actor.
      Task { @MainActor in self?.closePanel() }
    }
  }

  /// Stops watching for clicks outside, once the panel has closed.
  private func stopWatchingOutsideClicks() {
    if let outsideClicks { NSEvent.removeMonitor(outsideClicks) }
    outsideClicks = nil
  }

  /// The panel closed: reading slows to the closed rate, unless it opened again while it closed.
  ///
  /// - Parameter notification: The popover's notification.
  func popoverDidClose(_ notification: Notification) {
    guard !popover.isShown else { return }
    stopWatchingOutsideClicks()
    controller.watch(open: false)
  }
}
