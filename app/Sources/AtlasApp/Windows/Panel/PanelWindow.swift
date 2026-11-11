import AppKit
import AtlasUI
import SwiftUI

/// The menu bar panel's window, opened from Atlas's menu bar item (``StatusItem``), whose glyph it keeps on the
/// helper's mode. Borderless and clear, it hangs under the item and draws its own card, so it has no arrow, and it
/// opens at once and fades away, as a menu does (`PanelWindow+Closing`), where a popover would zoom out of the item
/// and back. The panel reads the picture only while it is open, opens its ⋯ menu here, under its button's trailing
/// edge, and answers its shortcuts while it is open (``PanelKeys``).
final class PanelWindow: NSPanel {
  /// 6: between ⋯ and the menu it opens, as in the main window.
  static let menuGap: CGFloat = 6
  /// 53: the Escape key's code.
  static let escapeKey: UInt16 = 53

  /// The panel's controller.
  let controller: PanelController
  /// Opens the Settings window, from the ⋯ menu.
  let openSettings: @MainActor () -> Void
  /// Opens the setup window, from the ⋯ menu.
  let gettingStarted: @MainActor () -> Void
  /// The menu bar item it opens from.
  let item: StatusItem
  /// Whether the panel shows: from its opening until it starts to close, so a panel fading away is already closed.
  var isOpen = false
  /// Counts openings and closings, so a fade that another opening or closing overtook ends without closing anything.
  var turn = 0
  /// When the panel last closed, in seconds since the Mac started; nil before it first closes.
  var closedAt: TimeInterval?
  /// Watches for clicks outside Atlas while the panel shows, so a click anywhere else closes it as a menu would;
  /// nil while it is closed. A menu bar app is often not the active one, so its own windows hear no such click.
  var outsideClicks: Any?
  /// Watches for clicks in Atlas's own windows while the panel shows, which close it too; nil while it is closed.
  var ownClicks: Any?
  /// Watches for Atlas no longer being the app in front while the panel shows; nil while it is closed.
  var leaving: (any NSObjectProtocol)?
  /// Answers the panel's shortcuts (``PanelKeys``) while the panel shows; nil while it is closed.
  var keys: Any?

  /// Opens from the item in the menu bar and starts reading at the closed rate.
  ///
  /// - Parameters:
  ///   - controller: The panel's controller.
  ///   - item: The menu bar item the panel opens from.
  ///   - openSettings: Opens the Settings window, from the ⋯ menu.
  ///   - gettingStarted: Opens the setup window, from the ⋯ menu.
  init(
    controller: PanelController,
    item: StatusItem,
    openSettings: @escaping @MainActor () -> Void,
    gettingStarted: @escaping @MainActor () -> Void
  ) {
    self.controller = controller
    self.item = item
    self.openSettings = openSettings
    self.gettingStarted = gettingStarted
    super.init(contentRect: .zero, styleMask: [.borderless], backing: .buffered, defer: false)
    // Over the menu bar's own level and under a menu's, so its ⋯ menu opens over it.
    level = .statusBar
    collectionBehavior = [.moveToActiveSpace, .fullScreenAuxiliary, .transient, .ignoresCycle]
    // It comes and goes its own way: at once, and fading.
    animationBehavior = .none
    isMovable = false
    isReleasedWhenClosed = false
    hidesOnDeactivate = false
    // Its heading is the status, so the window carries Atlas's name for VoiceOver.
    title = PanelWords.name
    // The window follows the panel as Recent and the line come and go (`PanelWindow+Resize`); like Atlas's other
    // windows, it follows the Mac's appearance.
    WindowShell.hostOverlay(in: self) { [weak self] in
      PanelView(state: controller.state) { self?.handle($0) }
        .panelWindowCard { self?.fit($0) }
    }
    item.button?.target = self
    item.button?.action = #selector(toggle)
    StateTracking.follow(
      { [weak controller] in controller?.glyph },
      apply: { [weak self] glyph in
        self?.item.show(glyph.kind, description: PanelWords.statusItem(glyph.word))
      }
    )
    // Hands VoiceOver each state that asks something of the person while the panel is open, once it holds.
    StateTracking.follow(
      { [weak controller] () -> String?? in
        guard let controller else { return nil }
        return .some(controller.askingStatus)
      },
      apply: { [weak controller] (text: String?) in controller?.announceSoon(text) }
    )
    controller.watch(open: false)
  }

  /// A borderless window takes the keyboard only when it says it can: the panel takes it, as a menu does, so Tab
  /// reaches its buttons.
  override var canBecomeKey: Bool { true }

  /// Whatever cancels, as VoiceOver's Escape does, closes the panel.
  ///
  /// - Parameter sender: What asked.
  override func cancelOperation(_ sender: Any?) {
    closePanel()
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
    if isOpen {
      closePanel()
    } else if PanelToggle.opens(closedAt: closedAt, now: ProcessInfo.processInfo.systemUptime) {
      openPanel()
    }
  }

  /// Opens the panel under its item, whole and at once, at the size its view takes now.
  private func openPanel() {
    guard let frame = frame(card: fittingHeight) else { return }
    turn += 1
    isOpen = true
    show(opacity: 1, over: 0)
    setFrame(frame, display: true)
    // The panel takes the keyboard, as a menu does, so Escape and its shortcuts reach it.
    NSApp.activate()
    makeKeyAndOrderFront(nil)
    // No button is focused until the person presses Tab, as in menus.
    makeFirstResponder(nil)
    // Again after this turn, once the window has come forward, and the item stays highlighted, as an open menu's
    // does, once the click that opened it has let the button go. Unstructured, as opening is synchronous.
    Task { [weak self] in
      self?.makeFirstResponder(nil)
      if self?.isOpen == true { self?.item.showsPanel(true) }
    }
    watchClicks()
    watchKeys()
    controller.watch(open: true)
  }

  /// The height the panel's view takes for what it shows now, measured off screen: the window's own view lays out
  /// only while it shows, so what it last said may be a state behind.
  private var fittingHeight: CGFloat {
    let state = controller.state
    let view = ShellRoot {
      PanelView(state: state) { _ in }
        .fixedSize(horizontal: false, vertical: true)
    }
    return WindowShell.hostingView(view).fittingSize.height
  }

  /// Where the window goes for a card of a height: hanging from the menu bar under the item.
  ///
  /// - Parameter height: The card's height.
  /// - Returns: The window's frame on the item's screen, or nil while the item is in no menu bar.
  private func frame(card height: CGFloat) -> NSRect? {
    guard let anchor, let bar = item.button?.window else { return nil }
    return PanelPlacement.frame(
      size: PanelWindowFrame.size(card: height),
      under: anchor,
      in: bar.screen?.visibleFrame ?? anchor,
      room: PanelWindowFrame.side
    )
  }

  /// The menu bar item's frame on its screen, or nil while it is in no menu bar.
  var anchor: NSRect? {
    guard let button = item.button, let bar = button.window else { return nil }
    return bar.convertToScreen(button.convert(button.bounds, to: nil))
  }

  /// The glyph the menu bar item draws.
  var icon: MenuBarIcon { item.icon }

  /// Draws the menu bar item as another glyph, at once.
  ///
  /// - Parameter icon: The glyph.
  func use(_ icon: MenuBarIcon) {
    item.use(icon)
  }

  /// What the ⋯ menu's items and the panel's shortcuts do.
  private var menuActions: PanelMoreMenu.Actions {
    PanelMoreMenu.Actions(
      openAtlas: { [weak self] in self?.controller.send(.openAtlas) },
      openSettings: { [weak self] in
        self?.closePanel()
        self?.openSettings()
      },
      gettingStarted: { [weak self] in
        self?.closePanel()
        self?.gettingStarted()
      },
      quit: { [weak self] in self?.controller.links.quit() }
    )
  }

  /// Opens the ⋯ menu under its button, its trailing edge on the button's, the button pressed until the menu closes.
  ///
  /// - Parameter frame: The button's frame in the window, top left first.
  private func openMore(below frame: CGRect) {
    guard let content = contentView else { return }
    let menu = PanelMoreMenu.menu(controller.moreMenu, actions: menuActions)
    controller.setMoreOpen(true)
    // SwiftUI's frames run down from the top; a view that is not flipped runs up from the bottom.
    let below = frame.maxY + Self.menuGap
    let point = NSPoint(
      x: frame.maxX - menu.size.width,
      y: content.isFlipped ? below : content.bounds.height - below
    )
    menu.popUp(positioning: nil, at: point, in: content)
    controller.setMoreOpen(false)
  }

  /// Answers the panel's keys while the panel shows and its ⋯ menu is closed: Escape closes it, as it closes a menu,
  /// ⌘O and ⌘, act as the menu's items would, ⌘Q closes all windows, and ⌘. and ⇧⌘P act as the buttons they name
  /// would. Other keys pass on.
  private func watchKeys() {
    stopWatchingKeys()
    keys = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
      // A local monitor runs on the main thread, as the event loop does.
      let answered = MainActor.assumeIsolated { self?.answer(event) ?? false }
      return answered ? nil : event
    }
  }

  /// Answers a key press while the panel shows and its ⋯ menu is closed: Escape, or one of the panel's shortcuts.
  ///
  /// - Parameter event: The key press.
  /// - Returns: Whether the panel answered it, so the key goes no further.
  private func answer(_ event: NSEvent) -> Bool {
    if event.keyCode == Self.escapeKey, isOpen, !controller.session.moreOpen {
      closePanel()
      return true
    }
    let shortcut = PanelKeys.shortcut(characters: event.charactersIgnoringModifiers, modifiers: event.modifierFlags)
    return shortcut.map { answer($0) } ?? false
  }

  /// Runs one of the panel's shortcuts while the panel shows and its ⋯ menu is closed.
  ///
  /// - Parameter shortcut: What the key asked for.
  /// - Returns: Whether the panel ran it, so the key goes no further.
  func answer(_ shortcut: PanelKeys.Shortcut) -> Bool {
    guard isOpen, !controller.session.moreOpen else { return false }
    let actions = menuActions
    switch shortcut {
    case .openAtlas: actions.openAtlas()
    case .openSettings: actions.openSettings()
    case .closeWindows: controller.links.closeWindows()
    case .stopOrResume, .pausePicture: return controller.press(shortcut)
    }
    return true
  }

  /// Stops answering the panel's shortcuts, once the panel closes.
  func stopWatchingKeys() {
    if let keys { NSEvent.removeMonitor(keys) }
    keys = nil
  }
}
