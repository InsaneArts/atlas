import AppKit
import AtlasUI
import SwiftUI

/// The main window: dark, resizable, its one quiet row in the title bar. It reads only while on screen, gives
/// control back whenever it closes, hides or loses focus, opens the top row's menus, hands a dragged screenshot to
/// the system, reads the Screenshots tab's keys (``ShotKeys``), and answers the main menu's View, File and Helper
/// items (`ViewerWindow+Menus`) as the window itself, which the responder chain reaches. Built on the one shell
/// (``WindowShell``), and opened and closed by the coordinator (``WindowPresenting``).
final class ViewerWindow: NSWindow, NSWindowDelegate, WindowPresenting {
  /// The name its frame is saved under. Shipped, so it never changes.
  static let autosaveName = "AtlasViewer"
  /// 1200 by 800: the window's size the first time, until the person moves or resizes it.
  static let firstSize = NSSize(width: 1200, height: 800)
  /// 860 by 520: the least the window shrinks to, where the top row, drawn in as far as it goes
  /// (`ViewerLayout.topRowFit`), still holds the status word, the four tabs and the widest set of buttons (Resume
  /// beside Take, then ⋯ and Details) with its 14-point inset, measured from the renders of Working, Stopped and You
  /// have control; Stopped needs about 852.
  static let leastSize = NSSize(width: 860, height: 520)
  /// 6: between a top-row button and the menu it opens.
  static let menuGap: CGFloat = 6

  /// The window's controller.
  let controller: ViewerController
  /// Runs once the window has closed.
  let closed: @MainActor (ViewerWindow) -> Void
  /// Open until closed, minimised included.
  private(set) var isPresented = false
  /// The empty toolbar that lines the window buttons up with the top row; taken off in full screen, where it would
  /// become the auto-hiding full-screen toolbar and cover the row.
  private let rowToolbar = NSToolbar(identifier: "viewer")
  /// Reads the Screenshots tab's keys while the window is open.
  private var shotKeys: Any?

  /// Creates the window, hidden until ``present()``.
  ///
  /// - Parameters:
  ///   - controller: The window's controller.
  ///   - closed: Runs once the window has closed.
  init(controller: ViewerController, closed: @escaping @MainActor (ViewerWindow) -> Void) {
    self.controller = controller
    self.closed = closed
    super.init(
      contentRect: NSRect(origin: .zero, size: Self.firstSize),
      styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
      backing: .buffered,
      defer: false
    )
    // The title is for the Window menu alone.
    title = ViewerWords.windowTitle
    titleVisibility = .hidden
    titlebarAppearsTransparent = true
    titlebarSeparatorStyle = .none
    // An empty toolbar gives the title bar the top row's height, so the window buttons line up with it.
    toolbar = rowToolbar
    toolbarStyle = .unified
    isReleasedWhenClosed = false
    contentMinSize = Self.leastSize
    collectionBehavior.insert(.fullScreenPrimary)
    delegate = self
    WindowShell.host(in: self) { [weak self] in
      ViewerView(
        state: controller.state,
        send: { self?.handle($0) },
        newest: { controller.newestPicture },
        input: { DesktopInput(controller: controller) }
      )
      .environment(
        \.shotDrag,
        ShotDragSource { name in
          ShotDrag.provider(
            name: name,
            suggestedName: ViewerPresenter.shotFileName(name, controller.snapshot),
            service: controller.service
          )
        }
      )
    }
    setFrameAutosaveName(Self.autosaveName)
  }

  /// Brings the window forward, where it was left, and reads while it is on screen. The app shows its Dock icon
  /// first (``DockIcon``). Ordered front is not on screen: while the screen is locked or asleep the window is
  /// occluded, and reading starts with the occlusion change once it shows.
  func present() {
    if !isPresented && !setFrameUsingName(Self.autosaveName) { center() }
    isPresented = true
    watchShotKeys()
    if isMiniaturized { deminiaturize(nil) }
    makeKeyAndOrderFront(nil)
    NSApp.activate()
    if isKeyWindow { controller.gainedFocus() } else { controller.lostFocus() }
    followScreen()
  }

  /// What the person did in the window: Keys and ⋯ open their menus here, everything else goes to the controller.
  ///
  /// - Parameter event: The event.
  func handle(_ event: ViewerEvent) {
    switch event {
    case .showKeys(let frame):
      open(KeysMenu.menu(for: controller), below: frame, as: .keys)
    case .showMore(let frame):
      open(MoreMenu.menu(controller.moreMenu, target: self), below: frame, as: .more)
    default:
      controller.send(event)
    }
  }

  /// Opens a menu under a top-row button, its button pressed until the menu closes.
  ///
  /// - Parameters:
  ///   - menu: The menu.
  ///   - frame: The button's frame in the window's content, top left first.
  ///   - kind: Which menu.
  private func open(_ menu: NSMenu, below frame: CGRect, as kind: ViewerMenu) {
    guard let content = contentView else { return }
    controller.setOpenMenu(kind)
    // The hosting view is flipped, as SwiftUI's frames are: down is a larger y.
    let point = NSPoint(x: frame.minX, y: frame.maxY + Self.menuGap)
    menu.popUp(positioning: nil, at: point, in: content)
    controller.setOpenMenu(nil)
  }

  /// Reads the Screenshots tab's keys before the window does: Space, ← and → and Esc, unless a field takes the keys or
  /// an alert is up.
  private func watchShotKeys() {
    guard shotKeys == nil else { return }
    // AppKit runs a local monitor's handler on the main thread, and its handler type is not `@Sendable`, so the
    // handler shares this window's main-actor isolation and reads the event where it was delivered.
    shotKeys = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
      guard let self else { return event }
      // nil means handled here: AppKit then does nothing more with it.
      return shotKey(event)
    }
  }

  /// A key on the Screenshots tab: its event goes to the controller, and the key goes no further.
  ///
  /// - Parameter event: The key.
  /// - Returns: The key, to pass on, or nil once handled.
  private func shotKey(_ event: NSEvent) -> NSEvent? {
    guard event.window === self, controller.session.tab == .screenshots, controller.state.alert == nil,
      !(firstResponder is NSText)
    else { return event }
    let held = event.modifierFlags.intersection([.command, .control, .option])
    let largeShown = controller.panes.shots.large != nil
    guard let shot = ShotKeys.event(keyCode: event.keyCode, hasModifiers: !held.isEmpty, largeShown: largeShown)
    else { return event }
    controller.send(shot)
    return nil
  }

  /// Reads only while the window is on screen: off screen (minimised, covered, on another Space, or behind a locked
  /// or sleeping screen) it stops reading and gives control back; on screen again, it reads again.
  func followScreen() {
    controller.setVisible(occlusionState.contains(.visible) && isVisible)
  }

  /// Full screen hides the window buttons the top row keeps clear of. The empty toolbar comes off first: kept, it
  /// becomes the full-screen toolbar that slides away with the menu bar and takes the top row with it, so the row
  /// lays out alone at the top at its own height.
  ///
  /// - Parameter notification: The window's notification.
  func windowWillEnterFullScreen(_ notification: Notification) {
    toolbar = nil
    controller.setFullScreen(true)
  }

  /// Out of full screen, the window buttons and the toolbar that lines them up with the top row are back.
  ///
  /// - Parameter notification: The window's notification.
  func windowDidExitFullScreen(_ notification: Notification) {
    toolbar = rowToolbar
    controller.setFullScreen(false)
  }

  /// Full screen did not happen after all: the toolbar goes back, as on leaving it.
  ///
  /// - Parameter window: The window.
  func windowDidFailToEnterFullScreen(_ window: NSWindow) {
    toolbar = rowToolbar
    controller.setFullScreen(false)
  }

  /// Closes the window.
  func dismiss() { close() }

  /// The window went on or off screen: reading follows (``followScreen()``).
  ///
  /// - Parameter notification: The window's notification.
  func windowDidChangeOcclusionState(_ notification: Notification) { followScreen() }

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
    isPresented = false
    if let shotKeys { NSEvent.removeMonitor(shotKeys) }
    shotKeys = nil
    controller.close()
    // Unstructured, so `closed` runs once AppKit has finished closing the window, not during its notification.
    Task { closed(self) }
  }
}
