import AppKit
import AtlasControl

/// The keyboard's side: in control, a key goes to the input method, and a command it names goes to the helper as
/// its key.
extension DesktopCanvasView {
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
