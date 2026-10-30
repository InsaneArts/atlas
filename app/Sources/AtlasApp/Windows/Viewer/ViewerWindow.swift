import AppKit
import AtlasUI
import SwiftUI

/// The main window: resizable to any size, with no least size of its own, the sidebar under its window buttons and the
/// content card beside it, which give way to its size as the controller fits them (``ViewerController/fitWindow(to:)``),
/// titled with the helper's name for the Window menu. It reads only while on screen, gives control back whenever it closes, hides or
/// loses focus, hands a dragged screenshot to the system, takes the keyboard back from the search field on a press
/// anywhere else in it (``FieldFocus``), reads the Screenshots
/// list's keys (``ShotKeys``) and Escape for a window outlined on the Screen, hands VoiceOver the states that ask
/// something of the person (``ViewerController/askingStatus``), and answers the main menu's View, File, Edit and
/// Helper items (`ViewerWindow+Menus`) as the window itself, which the responder chain reaches. Built on the one
/// shell (``WindowShell``), and opened and closed by the coordinator (``WindowPresenting``).
final class ViewerWindow: NSWindow, NSWindowDelegate, WindowPresenting {
  /// The name its frame is saved under. Shipped, so it never changes.
  static let autosaveName = "AtlasViewer"
  /// 1200 by 800: the window's size the first time, until the person moves or resizes it.
  static let firstSize = NSSize(width: 1200, height: 800)
  /// 0.9: the share of a small display's room the window takes the first time, at most.
  static let firstShare: CGFloat = 0.9

  /// The window's controller.
  let controller: ViewerController
  /// Runs once the window has closed.
  let closed: @MainActor (ViewerWindow) -> Void
  /// Open until closed, minimised included.
  private(set) var isPresented = false
  /// The empty toolbar that gives the title bar its height, so the window buttons line up with the card's header;
  /// taken off in full screen, where it would become the auto-hiding full-screen toolbar.
  private let rowToolbar = NSToolbar(identifier: "viewer")
  /// Whether a screenshot surface itself owns focus; read-only outside the window, including native dispatch tests.
  private(set) var screenshotFocused = false
  /// Takes the keyboard back from the search field on a press anywhere else in the window.
  private let fieldFocus = FieldFocus()

  /// Creates the window, hidden until ``present()``.
  ///
  /// - Parameters:
  ///   - controller: The window's controller.
  ///   - closed: Runs once the window has closed.
  init(controller: ViewerController, closed: @escaping @MainActor (ViewerWindow) -> Void) {
    self.controller = controller
    self.closed = closed
    super.init(
      contentRect: NSRect(origin: .zero, size: Self.openingSize(on: NSScreen.main)),
      styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
      backing: .buffered,
      defer: false
    )
    // The title is for the Window menu alone.
    title = ViewerWords.windowTitle
    titleVisibility = .hidden
    titlebarAppearsTransparent = true
    titlebarSeparatorStyle = .none
    // An empty toolbar gives the title bar its height, so the window buttons line up with the card's header.
    toolbar = rowToolbar
    toolbarStyle = .unified
    isReleasedWhenClosed = false
    // The system's zoom in and shrink out, as a document opens and closes; AppKit honours Reduce Motion.
    animationBehavior = .documentWindow
    collectionBehavior.insert(.fullScreenPrimary)
    // The frame it was left at comes back now, so the content is fitted to it before it is first drawn and never
    // opens with the sidebar only to hide it.
    setFrameAutosaveName(Self.autosaveName)
    controller.fitWindow(to: frame.size)
    delegate = self
    let fieldReport = fieldFocus.report
    let screenshotReport = ScreenshotFocusReport { [weak self] focused in
      self?.screenshotFocused = focused
    }
    WindowShell.host(in: self) {
      ViewerView(
        state: controller.state,
        send: { controller.send($0) },
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
      .environment(\.focusedFieldReport, fieldReport)
      .environment(\.screenshotFocusReport, screenshotReport)
    }
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
  /// icon first (``DockIcon``). Opening, its content is fitted to its frame, then lays out and draws before it shows, so the zoom carries
  /// the last picture the panel read rather than Connecting. Ordered front is not on screen: while the screen
  /// is locked or asleep the window is occluded, and reading starts with the occlusion change once it shows.
  func present() {
    if !isPresented && !setFrameUsingName(Self.autosaveName) { center() }
    controller.fitWindow(to: frame.size)
    if !isVisible {
      contentView?.layoutSubtreeIfNeeded()
      displayIfNeeded()
    }
    isPresented = true
    fieldFocus.watch(self)
    if isMiniaturized { deminiaturize(nil) }
    makeKeyAndOrderFront(nil)
    NSApp.activate()
    if isKeyWindow { controller.control.gainedFocus() } else { controller.control.lostFocus() }
    followScreen()
  }

  /// Dispatches screenshot keys only while their subtree owns focus; all other events reach the native responder.
  ///
  /// - Parameter event: The window event being dispatched.
  override func sendEvent(_ event: NSEvent) {
    if event.type == .keyDown, shotKey(event) == nil { return }
    super.sendEvent(event)
  }

  /// A key on the Screenshots list: its event goes to the controller, and the key goes no further.
  ///
  /// - Parameter event: The key.
  /// - Returns: The key, to pass on, or nil once handled.
  private func shotKey(_ event: NSEvent) -> NSEvent? {
    guard event.window === self, !(firstResponder is NSText) else { return event }
    let snapshot = controller.snapshot
    guard ViewerPresenter.alert(snapshot) == nil else { return event }
    let held = event.modifierFlags.intersection([.command, .control, .option])
    if snapshot.session.overlayShown {
      if let dismiss = ShotKeys.overlayEvent(
        keyCode: event.keyCode,
        hasModifiers: !held.isEmpty,
        holdingControl: snapshot.holdingControl
      ) {
        controller.send(dismiss)
        return nil
      }
      return event
    }
    if snapshot.session.place == .screen {
      return outlineKey(event, hasModifiers: !held.isEmpty, snapshot: snapshot)
    }
    guard snapshot.session.place == .screenshots, screenshotFocused else { return event }
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

  /// Full screen hides the window buttons the sidebar keeps clear of. The empty toolbar comes off first: kept, it
  /// becomes the full-screen toolbar that slides away with the menu bar, so the content lays out alone from the top.
  ///
  /// - Parameter notification: The window's notification.
  func windowWillEnterFullScreen(_ notification: Notification) {
    toolbar = nil
    controller.setFullScreen(true)
  }

  /// Out of full screen, the window buttons and the toolbar that lines them up with the card's header are back.
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

  /// The window's size the first time: ``firstSize``, or on a small display at most ``firstShare`` of its room.
  ///
  /// - Parameter screen: The display it opens on; none keeps ``firstSize``.
  /// - Returns: The size.
  static func openingSize(on screen: NSScreen?) -> NSSize {
    guard let room = screen?.visibleFrame.size else { return firstSize }
    return NSSize(
      width: min(firstSize.width, (room.width * firstShare).rounded(.down)),
      height: min(firstSize.height, (room.height * firstShare).rounded(.down))
    )
  }

  /// The window changed size: the content fits it, changing only as the window crosses a threshold.
  ///
  /// - Parameter notification: The window's notification.
  func windowDidResize(_ notification: Notification) {
    controller.fitWindow(to: frame.size)
  }

  /// The person began resizing the window: what would move under the pointer holds still until they let go.
  ///
  /// - Parameter notification: The window's notification.
  func windowWillStartLiveResize(_ notification: Notification) {
    controller.setLiveResizing(true)
  }

  /// The person let go of the window's edge: the Screenshots grid settles its columns and the thumbnails read.
  ///
  /// - Parameter notification: The window's notification.
  func windowDidEndLiveResize(_ notification: Notification) {
    controller.setLiveResizing(false)
  }

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
    fieldFocus.stop()
    controller.close()
    // Unstructured, so `closed` runs once AppKit has finished closing the window, not during its notification.
    Task { closed(self) }
  }
}
