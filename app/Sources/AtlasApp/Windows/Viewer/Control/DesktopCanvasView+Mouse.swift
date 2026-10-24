import AppKit
import AtlasService

/// The mouse's side: presses, drags and releases of any button, movement and scrolling go to the helper while in
/// control, and a right click never opens a menu here.
extension DesktopCanvasView {
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
    controller.control.pointerDown(button, at: location(event), in: bounds.size, modifiers: event.heldModifiers)
  }

  /// A held button moved.
  ///
  /// - Parameter event: The event.
  private func dragged(_ event: NSEvent) {
    guard let button = Self.button(event) else { return }
    controller?.control.pointerDragged(button, to: location(event), in: bounds.size, modifiers: event.heldModifiers)
  }

  /// A button came up.
  ///
  /// - Parameter event: The event.
  private func up(_ event: NSEvent) {
    guard let button = Self.button(event) else { return }
    controller?.control.pointerUp(button, at: location(event), in: bounds.size, modifiers: event.heldModifiers)
  }

  /// The pointer moved with no button held.
  ///
  /// - Parameter event: The event.
  override func mouseMoved(with event: NSEvent) {
    controller?.control.pointerMoved(to: location(event), in: bounds.size)
  }

  /// Scrolling goes to the helper while in control, and to the window otherwise.
  ///
  /// - Parameter event: The event.
  override func scrollWheel(with event: NSEvent) {
    guard let controller, controller.holdingControl, window?.isKeyWindow == true else {
      return super.scrollWheel(with: event)
    }
    controller.control.scrolled(
      dx: event.scrollingDeltaX,
      dy: event.scrollingDeltaY,
      precise: event.hasPreciseScrollingDeltas,
      began: event.phase.contains(.began),
      at: location(event),
      in: bounds.size
    )
  }

  /// A right click goes to the helper, never to a menu here.
  ///
  /// - Parameter event: The click.
  /// - Returns: No menu.
  override func menu(for event: NSEvent) -> NSMenu? { nil }
}
