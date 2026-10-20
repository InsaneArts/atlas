/// Where a key pressed while in control goes, before menus and the window's own shortcuts see it: giving control
/// back works at any moment, quitting stays with this Mac, keys mid-composition belong to the input method, and
/// everything else goes to the helper.
package enum KeyRoute: Equatable, Sendable {
  /// Control-Shift-Escape: control goes back, mid-composition included, and the key goes no further.
  case release
  /// The key stays on this Mac: Command-Q, by the key labelled Q in any layout, or a key the input method composes
  /// with.
  case passOn
  /// The key goes to the helper as a key, a paste or text; a shortcut mid-composition ends the composition first.
  case send(endingComposition: Bool)

  /// Routes one key press.
  ///
  /// - Parameters:
  ///   - keyCode: The Mac virtual key code pressed.
  ///   - modifiers: The modifier keys held.
  ///   - typesQ: Whether the key types "q" in this Mac's layout, whatever its position.
  ///   - composing: Whether the input method is composing text now.
  package init(keyCode: UInt16, modifiers: Modifiers, typesQ: Bool, composing: Bool) {
    if KeyMap.isRelease(keyCode: keyCode, modifiers: modifiers) {
      self = .release
    } else if modifiers == .command && typesQ {
      self = .passOn
    } else if composing && !modifiers.contains(.command) {
      self = .passOn
    } else {
      self = .send(endingComposition: composing)
    }
  }

  /// Whether a key down is a repeat of Control-Shift-Escape, held down: the press itself gives control back or takes
  /// it, and its repeats do nothing at all, so holding the chord never takes back the control it just gave.
  ///
  /// - Parameters:
  ///   - keyCode: The Mac virtual key code pressed.
  ///   - modifiers: The modifier keys held.
  ///   - repeating: Whether the key down repeats a key held down.
  /// - Returns: Whether the key down is swallowed.
  package static func swallows(keyCode: UInt16, modifiers: Modifiers, repeating: Bool) -> Bool {
    repeating && KeyMap.isRelease(keyCode: keyCode, modifiers: modifiers)
  }

  /// Whether a key sent to the helper still goes on to this Mac afterwards: one the helper did not take as a key
  /// goes on as text, unless it is a shortcut, which is kept from this app.
  ///
  /// - Parameters:
  ///   - handled: Whether the key went to the helper as a key or a paste.
  ///   - modifiers: The modifier keys held.
  /// - Returns: Whether AppKit still handles the key.
  package static func passesOn(handled: Bool, modifiers: Modifiers) -> Bool {
    !handled && !modifiers.contains(.command)
  }
}
