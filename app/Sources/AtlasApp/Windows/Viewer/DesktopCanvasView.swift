import AppKit
import AtlasControl

/// The surface over the helper's picture that takes the mouse, scrolling
/// and the keyboard while this window holds control. Keys that name a key
/// (Return, arrows, shortcuts) go as keys; everything else goes through
/// the input method, so dead keys, accents and other scripts arrive as the
/// text they commit.
final class DesktopCanvasView: NSView {
  /// The window's controller, which every input goes to.
  weak var controller: ViewerController?
  /// The key monitor, while the view is in a window.
  private var keys: Any?
  /// The text the input method is composing.
  var marked = NSAttributedString()
  /// The input method's selection within the marked text.
  var markedSelection = NSRange(location: 0, length: 0)
  /// Whether a window takes the keyboard now: AppKit's own answer, which a test stands in for, as a window it
  /// draws off screen never becomes key.
  var takesKeyboard: @MainActor (NSWindow) -> Bool = { $0.isKeyWindow }

  /// Locations run from the top left, as the helper's display does.
  override var isFlipped: Bool { true }
  /// It takes the keyboard only while in control.
  override var acceptsFirstResponder: Bool { controller?.holdingControl == true }

  /// A click that brings the window forward is never sent to the helper.
  ///
  /// - Parameter event: The click.
  /// - Returns: False.
  override func acceptsFirstMouse(for event: NSEvent?) -> Bool { false }

  /// Tracks the pointer over the whole visible view while the window is key.
  override func updateTrackingAreas() {
    super.updateTrackingAreas()
    trackingAreas.forEach(removeTrackingArea)
    addTrackingArea(
      NSTrackingArea(
        rect: .zero,
        options: [.mouseMoved, .activeInKeyWindow, .inVisibleRect],
        owner: self
      )
    )
  }

  /// Intercepts keys while the view is in a window, and stops when it leaves its window or the window closes, so the
  /// key monitor never outlives the window, whichever goes first.
  override func viewDidMoveToWindow() {
    super.viewDidMoveToWindow()
    let center = NotificationCenter.default
    center.removeObserver(self, name: NSWindow.willCloseNotification, object: nil)
    guard let window else { return stopIntercepting() }
    center.addObserver(
      self,
      selector: #selector(windowWillClose),
      name: NSWindow.willCloseNotification,
      object: window
    )
    startIntercepting()
  }

  /// The window is closing: the key monitor goes.
  @objc private func windowWillClose() { stopIntercepting() }

  /// Follow control: take the keyboard while it is held, give it up after.
  func follow() {
    guard let window, let controller else { return }
    if controller.holdingControl {
      if window.firstResponder !== self { window.makeFirstResponder(self) }
    } else {
      if hasMarkedText() { unmarkText() }
      if window.firstResponder === self { window.makeFirstResponder(nil) }
    }
  }

  /// Keys are seen before menus and the window's own shortcuts: while in control, so a shortcut, the main menu's
  /// ⌘1 to ⌘4, ⌘. and ⌘S included, reaches the helper rather than this app, and always, so a held
  /// Control-Shift-Escape's repeats reach neither. Only Control-Shift-Escape, which gives control back, and Command-Q
  /// stay with this Mac (``KeyRoute``).
  private func startIntercepting() {
    guard keys == nil else { return }
    // AppKit runs a local monitor's handler on the main thread, and its handler type is not `@Sendable`, so the
    // handler shares this view's main-actor isolation and reads the event where it was delivered.
    keys = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
      guard let self else { return event }
      // nil means handled here: AppKit then does nothing more with it.
      return intercept(event)
    }
  }

  /// Removes the key monitor.
  func stopIntercepting() {
    guard let keys else { return }
    NSEvent.removeMonitor(keys)
    self.keys = nil
  }

  /// Becoming first responder connects the keyboard.
  ///
  /// - Returns: Whether it became first responder.
  override func becomeFirstResponder() -> Bool {
    let became = super.becomeFirstResponder()
    if became { controller?.setKeyboardConnected(true) }
    return became
  }

  /// Resigning disconnects the keyboard.
  ///
  /// - Returns: Whether it resigned.
  override func resignFirstResponder() -> Bool {
    let resigned = super.resignFirstResponder()
    if resigned { controller?.setKeyboardConnected(false) }
    return resigned
  }

  /// What the key monitor does with a key down in this window.
  ///
  /// - Parameter event: The key down.
  /// - Returns: The event to pass on, or nil when handled here.
  func intercept(_ event: NSEvent) -> NSEvent? {
    guard let window, takesKeyboard(window) else { return event }
    let modifiers = event.heldModifiers
    // Giving control back by holding ⌃⇧⎋ ends the key monitor's control here, and the control button's shortcut would
    // take each repeat as Take Control: the repeats go nowhere, in control or not.
    if KeyRoute.swallows(keyCode: event.keyCode, modifiers: modifiers, repeating: event.isARepeat) { return nil }
    guard let controller, controller.holdingControl, window.firstResponder === self else { return event }
    let route = KeyRoute(
      keyCode: event.keyCode,
      modifiers: modifiers,
      typesQ: event.charactersIgnoringModifiers?.lowercased() == "q",
      composing: hasMarkedText()
    )
    switch route {
    case .release:
      if hasMarkedText() { unmarkText() }
      controller.releaseControl()
      return nil
    case .passOn:
      return event
    case .send(let endingComposition):
      if endingComposition { unmarkText() }
      let handled = controller.keyPressed(keyCode: event.keyCode, modifiers: modifiers) {
        NSPasteboard.general
          .string(forType: .string)
      }
      return KeyRoute.passesOn(handled: handled, modifiers: modifiers) ? event : nil
    }
  }

  /// Where an event happened, from the view's top left.
  ///
  /// - Parameter event: A mouse event.
  /// - Returns: Its location in the view.
  func location(_ event: NSEvent) -> CGPoint { convert(event.locationInWindow, from: nil) }
}
