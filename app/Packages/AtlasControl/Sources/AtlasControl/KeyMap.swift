import AtlasService

/// Keys as the service names them, from Mac virtual key codes.
public enum KeyMap {
  /// Keys that are always sent as keys, never as text.
  static let named: [UInt16: String] = [
    36: "return", 76: "return", 48: "tab", 51: "backspace", 117: "forwarddelete", 53: "escape",
    123: "left", 124: "right", 125: "down", 126: "up",
    115: "home", 119: "end", 116: "pageup", 121: "pagedown",
    122: "f1", 120: "f2", 99: "f3", 118: "f4", 96: "f5", 97: "f6",
    98: "f7", 100: "f8", 101: "f9", 109: "f10", 103: "f11", 111: "f12",
  ]

  /// The key at each position on the keyboard, as on a US layout: a
  /// shortcut names the key pressed, whatever the layout prints on it.
  static let positions: [UInt16: String] = [
    0: "a", 11: "b", 8: "c", 2: "d", 14: "e", 3: "f", 5: "g", 4: "h", 34: "i", 38: "j", 40: "k",
    37: "l", 46: "m", 45: "n", 31: "o", 35: "p", 12: "q", 15: "r", 1: "s", 17: "t", 32: "u", 9: "v",
    13: "w", 7: "x", 16: "y", 6: "z",
    29: "0", 18: "1", 19: "2", 20: "3", 21: "4", 23: "5", 22: "6", 26: "7", 28: "8", 25: "9",
    33: "[", 30: "]", 42: "\\", 44: "/", 27: "-", 24: "=", 41: ";", 39: "'", 50: "`", 43: ",",
    47: ".", 49: "space",
    // The keypad, as the main-row keys that type the same: the service names no keypad key, and keypad Enter is
    // Return, above. Keypad Clear, and the § key of ISO keyboards, have no name the service knows: they send nothing.
    82: "0", 83: "1", 84: "2", 85: "3", 86: "4", 87: "5", 88: "6", 89: "7", 91: "8", 92: "9",
    65: ".", 75: "/", 78: "-", 81: "=",
  ]

  /// The keypad keys whose character is a main-row key's shifted one on a US layout: * is Shift-8 and + is
  /// Shift-=, as the service itself reads `cmd++`.
  static let shiftedPositions: [UInt16: String] = [67: "8", 69: "="]

  /// 9: the key labelled V on a US layout, the paste gesture's key at any layout's same position.
  static let vKey: UInt16 = 9
  /// 53: Escape.
  static let escapeKey: UInt16 = 53

  /// The key action for a key press, or nil when it is text (which
  /// arrives committed, through the input method) or nothing at all.
  ///
  /// - Parameters:
  ///   - keyCode: The Mac virtual key code pressed.
  ///   - modifiers: The modifier keys held.
  /// - Returns: A `key` action such as `cmd+a`, or `nil` when the press is text or no key.
  public static func action(keyCode: UInt16, modifiers: Modifiers) -> InputAction? {
    if let key = named[keyCode] { return chord(key, modifiers) }
    guard modifiers.contains(.command) || modifiers.contains(.control) else { return nil }
    if let key = shiftedPositions[keyCode] { return chord(key, modifiers.union(.shift)) }
    return positions[keyCode].map { chord($0, modifiers) }
  }

  /// A key with the modifiers held, as the service names the chord.
  ///
  /// - Parameters:
  ///   - key: The key's name.
  ///   - modifiers: The modifier keys held.
  /// - Returns: The `key` action, such as `cmd+shift+8`.
  private static func chord(_ key: String, _ modifiers: Modifiers) -> InputAction {
    .key((modifiers.names + [key]).joined(separator: "+"))
  }

  /// Only Command-V is the paste gesture: it sends this Mac's clipboard
  /// as typed text. Control-V and the shortcut menu send a key.
  ///
  /// - Parameters:
  ///   - keyCode: The Mac virtual key code pressed.
  ///   - modifiers: The modifier keys held.
  /// - Returns: Whether the press pastes this Mac's clipboard.
  public static func isPaste(keyCode: UInt16, modifiers: Modifiers) -> Bool {
    modifiers == .command && keyCode == vKey
  }

  /// Control-Shift-Escape as a menu writes it, the shortcut ``isRelease(keyCode:modifiers:)`` matches: the viewer's
  /// control button names it in its tooltip.
  public static let releaseSymbols = "⌃⇧⎋"

  /// Control-Shift-Escape gives control back.
  ///
  /// - Parameters:
  ///   - keyCode: The Mac virtual key code pressed.
  ///   - modifiers: The modifier keys held.
  /// - Returns: Whether the press gives control back.
  public static func isRelease(keyCode: UInt16, modifiers: Modifiers) -> Bool {
    keyCode == escapeKey && modifiers.contains(.control) && modifiers.contains(.shift)
  }
}
