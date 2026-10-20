import AppKit
import AtlasKit

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

  /// Keys are seen before menus and the window's own shortcuts: while in control, so a shortcut reaches the helper
  /// rather than this app, and always, so a held Control-Shift-Escape's repeats reach neither.
  private func startIntercepting() {
    guard keys == nil else { return }
    keys = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
      // Local monitors run on the main thread, so the event never crosses threads; AppKit's handler type is not
      // main-actor bound, so Swift cannot know that, and the event is marked as safe to hand to the main actor.
      nonisolated(unsafe) let event = event
      nonisolated(unsafe) var passed: NSEvent? = event
      MainActor.assumeIsolated {
        // nil means handled here: AppKit then does nothing more with it.
        if let self { passed = self.intercept(event) }
      }
      return passed
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
  private func intercept(_ event: NSEvent) -> NSEvent? {
    guard let window, window.isKeyWindow else { return event }
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
  private func location(_ event: NSEvent) -> CGPoint { convert(event.locationInWindow, from: nil) }

  /// The button a mouse event is about.
  ///
  /// - Parameter event: A mouse event.
  /// - Returns: Left, right or middle, or nil for another button.
  private static func button(_ event: NSEvent) -> MouseButton? {
    switch event.buttonNumber {
    case 0: .left
    case 1: .right
    case 2: .middle
    default: nil
    }
  }

  // MARK: Mouse

  /// The left button went down.
  ///
  /// - Parameter event: The event.
  override func mouseDown(with event: NSEvent) { down(event) }
  /// The right button went down.
  ///
  /// - Parameter event: The event.
  override func rightMouseDown(with event: NSEvent) { down(event) }
  /// Another button went down.
  ///
  /// - Parameter event: The event.
  override func otherMouseDown(with event: NSEvent) { down(event) }
  /// The left button dragged.
  ///
  /// - Parameter event: The event.
  override func mouseDragged(with event: NSEvent) { dragged(event) }
  /// The right button dragged.
  ///
  /// - Parameter event: The event.
  override func rightMouseDragged(with event: NSEvent) { dragged(event) }
  /// Another button dragged.
  ///
  /// - Parameter event: The event.
  override func otherMouseDragged(with event: NSEvent) { dragged(event) }
  /// The left button came up.
  ///
  /// - Parameter event: The event.
  override func mouseUp(with event: NSEvent) { up(event) }
  /// The right button came up.
  ///
  /// - Parameter event: The event.
  override func rightMouseUp(with event: NSEvent) { up(event) }
  /// Another button came up.
  ///
  /// - Parameter event: The event.
  override func otherMouseUp(with event: NSEvent) { up(event) }

  /// A button went down: in control, the desktop takes the keyboard and the press goes to the helper.
  ///
  /// - Parameter event: The event.
  private func down(_ event: NSEvent) {
    guard let controller, controller.holdingControl, window?.isKeyWindow == true, let button = Self.button(event)
    else {
      return
    }
    window?.makeFirstResponder(self)
    controller.pointerDown(button, at: location(event), in: bounds.size, modifiers: event.heldModifiers)
  }

  /// A held button moved.
  ///
  /// - Parameter event: The event.
  private func dragged(_ event: NSEvent) {
    guard let button = Self.button(event) else { return }
    controller?.pointerDragged(button, to: location(event), in: bounds.size, modifiers: event.heldModifiers)
  }

  /// A button came up.
  ///
  /// - Parameter event: The event.
  private func up(_ event: NSEvent) {
    guard let button = Self.button(event) else { return }
    controller?.pointerUp(button, at: location(event), in: bounds.size, modifiers: event.heldModifiers)
  }

  /// The pointer moved with no button held.
  ///
  /// - Parameter event: The event.
  override func mouseMoved(with event: NSEvent) {
    controller?.pointerMoved(to: location(event), in: bounds.size)
  }

  /// Scrolling goes to the helper while in control, and to the window otherwise.
  ///
  /// - Parameter event: The event.
  override func scrollWheel(with event: NSEvent) {
    guard let controller, controller.holdingControl, window?.isKeyWindow == true else {
      return super.scrollWheel(with: event)
    }
    controller.scrolled(
      dx: event.scrollingDeltaX,
      dy: event.scrollingDeltaY,
      precise: event.hasPreciseScrollingDeltas,
      at: location(event),
      in: bounds.size
    )
  }

  /// A right click goes to the helper, never to a menu here.
  ///
  /// - Parameter event: The click.
  /// - Returns: No menu.
  override func menu(for event: NSEvent) -> NSMenu? { nil }

  // MARK: Keyboard

  /// In control, a key goes to the input method, which commits text or names a command.
  ///
  /// - Parameter event: The key down.
  override func keyDown(with event: NSEvent) {
    guard controller?.holdingControl == true else { return super.keyDown(with: event) }
    inputContext?.handleEvent(event)
  }

  /// Sends a key the input method turned into a command, when the key names one: a key it turns into a command
  /// after composing (Return after a dead key, an arrow in Hangul) still goes as that key.
  ///
  /// - Parameter selector: The command.
  override func doCommand(by selector: Selector) {
    guard let event = NSApp.currentEvent, event.type == .keyDown,
      KeyMap.action(keyCode: event.keyCode, modifiers: event.heldModifiers) != nil
    else { return }
    _ = controller?.keyPressed(keyCode: event.keyCode, modifiers: event.heldModifiers) { nil }
  }
}
