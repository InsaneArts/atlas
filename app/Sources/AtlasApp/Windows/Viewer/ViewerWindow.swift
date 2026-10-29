import AppKit
import AtlasUI
import SwiftUI

/// The main window: dark, resizable, its one quiet row in the title bar, titled with the helper's name for the Window
/// menu. It reads only while on screen, gives control back whenever it closes, hides or loses focus, opens the top
/// row's menus under their buttons, hands a dragged screenshot to the system, reads the Screenshots
/// tab's keys (``ShotKeys``) and Escape for a window outlined on the Screen, hands VoiceOver the states that ask
/// something of the person (``ViewerController/askingStatus``), and answers the main menu's View, File, Edit and
/// Helper items (`ViewerWindow+Menus`) as the window itself, which the responder chain reaches. Built on the one
/// shell (``WindowShell``), and opened and closed by the coordinator (``WindowPresenting``).
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
    // The system's zoom in and shrink out, as a document opens and closes; AppKit honours Reduce Motion.
    animationBehavior = .documentWindow
    contentMinSize = Self.leastSize
    collectionBehavior.insert(.fullScreenPrimary)
    delegate = self
    WindowShell.host(in: self) { [weak self] in
      ViewerView(
        state: controller.state,
        send: { self?.handle($0) },
        newest: { controller.newestPicture },
        input: { DesktopInput(controller: controller) },
        livePicture: { LayerPictureView(live: $0) }
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
    followTitle()
    followAnnouncements()
  }

  /// Titles the window with the helper's name once it is known, so the Window menu tells it from the setup window.
  private func followTitle() {
    StateTracking.follow { [weak self] in
      self.map { window in
        let reading = window.controller.reading
        return reading.hasStatus
          ? ViewerWords.windowTitle(helper: ViewerPresenter.account(reading.status)) : ViewerWords.windowTitle
      }
    } apply: { [weak self] title in
      if self?.title != title { self?.title = title }
    }
  }

  /// Hands VoiceOver each state that asks something of the person, once it holds
  /// (``ViewerController/announceSoon(_:)``).
  private func followAnnouncements() {
    StateTracking.follow { [weak self] () -> String?? in
      guard let self else { return nil }
      return .some(controller.askingStatus)
    } apply: { [weak self] (text: String?) in
      self?.controller.announceSoon(text)
    }
  }

  /// Brings the window forward, where it was left, and reads while it is on screen. The app shows its Dock
  /// icon first (``DockIcon``). Opening, its content lays out and draws before it shows, so the zoom carries
  /// the last picture the panel read rather than Connecting. Ordered front is not on screen: while the screen
  /// is locked or asleep the window is occluded, and reading starts with the occlusion change once it shows.
  func present() {
    if !isPresented && !setFrameUsingName(Self.autosaveName) { center() }
    if !isVisible {
      contentView?.layoutSubtreeIfNeeded()
      displayIfNeeded()
    }
    isPresented = true
    watchShotKeys()
    if isMiniaturized { deminiaturize(nil) }
    makeKeyAndOrderFront(nil)
    NSApp.activate()
    if isKeyWindow { controller.control.gainedFocus() } else { controller.control.lostFocus() }
    followScreen()
  }

  /// What the person did in the window: Keys and ⋯ open their menus here; everything else goes to the controller.
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

  /// Opens a menu under a top-row button at the bar's trailing end, its trailing edge on the button's, the button
  /// pressed until the menu closes.
  ///
  /// - Parameters:
  ///   - menu: The menu.
  ///   - frame: The button's frame in the window's content, top left first.
  ///   - kind: Which menu.
  private func open(_ menu: NSMenu, below frame: CGRect, as kind: ViewerMenu) {
    guard let content = contentView else { return }
    // Nothing animates in the frame the menu's tracking loop starts, nor as it ends: an animation under way
    // then would stop halfway until the menu closes.
    var still = Transaction(animation: nil)
    still.disablesAnimations = true
    withTransaction(still) { controller.setOpenMenu(kind) }
    menu.popUp(positioning: nil, at: Self.menuOrigin(below: frame, width: menu.size.width), in: content)
    withTransaction(still) { controller.setOpenMenu(nil) }
  }

  /// Where a top-row menu opens: under its button, ``menuGap`` below it, its trailing edge on the button's, as the
  /// buttons sit at the bar's trailing end. The hosting view is flipped, as SwiftUI's frames are: down is a larger y.
  ///
  /// - Parameters:
  ///   - frame: The button's frame in the window's content, top left first.
  ///   - width: The menu's width.
  /// - Returns: The menu's top left corner.
  static func menuOrigin(below frame: CGRect, width: CGFloat) -> NSPoint {
    NSPoint(x: frame.maxX - width, y: frame.maxY + menuGap)
  }

  /// Reads the Screenshots tab's keys before the window does, and Escape on the Screen while a window is outlined,
  /// unless a field takes the keys or an alert is up.
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
    guard event.window === self, !(firstResponder is NSText) else { return event }
    let snapshot = controller.snapshot
    guard ViewerPresenter.alert(snapshot) == nil else { return event }
    let held = event.modifierFlags.intersection([.command, .control, .option])
    if snapshot.session.tab == .screen {
      return outlineKey(event, hasModifiers: !held.isEmpty, snapshot: snapshot)
    }
    guard snapshot.session.tab == .screenshots else { return event }
    let largeShown = snapshot.panes.shots.large != nil
    guard let shot = ShotKeys.event(keyCode: event.keyCode, hasModifiers: !held.isEmpty, largeShown: largeShown)
    else { return event }
    controller.send(shot)
    return nil
  }

  /// Escape on the Screen stops showing the window outlined there, as its label's × does; while this window holds
  /// control every key is the helper's.
  ///
  /// - Parameters:
  ///   - event: The key.
  ///   - hasModifiers: Whether ⌘, ⌃ or ⌥ is held with it.
  ///   - snapshot: What the window held when the key arrived.
  /// - Returns: The key, to pass on, or nil once handled.
  private func outlineKey(_ event: NSEvent, hasModifiers: Bool, snapshot: ViewerSnapshot) -> NSEvent? {
    let key = ShotKeys.screenEvent(
      keyCode: event.keyCode,
      hasModifiers: hasModifiers,
      holdingControl: snapshot.holdingControl,
      outlineShown: ViewerPresenter.outline(snapshot) != nil
    )
    guard let key else { return event }
    controller.send(key)
    return nil
  }

  /// Reads only while the window is on screen: off screen (minimised, covered, on another Space, or behind a locked
  /// or sleeping screen) it stops reading and gives control back; on screen again, it reads again.
  func followScreen() {
    controller.setVisible(WindowVisibility.shows(isVisible: isVisible, occlusionState: occlusionState))
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
    controller.control.gainedFocus()
  }

  /// The window lost the keyboard: control goes back.
  ///
  /// - Parameter notification: The window's notification.
  func windowDidResignKey(_ notification: Notification) {
    controller.control.lostFocus()
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
