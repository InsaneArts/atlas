import Testing

@testable import AtlasKit

@Suite("KeyMap")
struct KeyMapTests {
  @Test("plain text is left to the input method; named keys and shortcuts go as keys")
  func textAndKeys() {
    // "e" alone is text; Command-A and Shift-Left are keys.
    #expect(KeyMap.action(keyCode: 14, modifiers: []) == nil)
    #expect(KeyMap.action(keyCode: 14, modifiers: .option) == nil)
    #expect(KeyMap.action(keyCode: 0, modifiers: .command) == .key("cmd+a"))
    #expect(KeyMap.action(keyCode: 123, modifiers: .shift) == .key("shift+left"))
    #expect(KeyMap.action(keyCode: 36, modifiers: []) == .key("return"))
    #expect(KeyMap.action(keyCode: 122, modifiers: []) == .key("f1"))
    #expect(
      KeyMap.action(keyCode: 0, modifiers: [.command, .shift, .control, .option]) == .key("cmd+ctrl+alt+shift+a")
    )
  }

  @Test(
    "a shortcut names the key at its position, whatever the layout prints",
    arguments: [(UInt16(33), "["), (44, "/"), (24, "="), (49, "space")]
  )
  func positions(keyCode: UInt16, key: String) {
    #expect(KeyMap.action(keyCode: keyCode, modifiers: .command) == .key("cmd+\(key)"))
  }

  @Test(
    "a keypad shortcut names the main-row key that types the same, as the service names no keypad key",
    arguments: [
      (UInt16(82), "cmd+0"), (83, "cmd+1"), (84, "cmd+2"), (85, "cmd+3"), (86, "cmd+4"), (87, "cmd+5"),
      (88, "cmd+6"), (89, "cmd+7"), (91, "cmd+8"), (92, "cmd+9"), (65, "cmd+."), (75, "cmd+/"), (78, "cmd+-"),
      (81, "cmd+="), (67, "cmd+shift+8"), (69, "cmd+shift+="), (76, "cmd+return"),
    ]
  )
  func keypad(keyCode: UInt16, chord: String) {
    #expect(KeyMap.action(keyCode: keyCode, modifiers: .command) == .key(chord))
  }

  @Test("only Command-V pastes this Mac's clipboard")
  func paste() {
    #expect(KeyMap.isPaste(keyCode: 9, modifiers: .command))
    #expect(!KeyMap.isPaste(keyCode: 9, modifiers: .control))
    #expect(!KeyMap.isPaste(keyCode: 9, modifiers: [.command, .shift]))
    #expect(!KeyMap.isPaste(keyCode: 9, modifiers: [.command, .option]))
    #expect(KeyMap.action(keyCode: 9, modifiers: .control) == .key("ctrl+v"))
  }

  @Test("Control-Shift-Escape gives control back")
  func release() {
    #expect(KeyMap.isRelease(keyCode: 53, modifiers: [.control, .shift]))
    #expect(!KeyMap.isRelease(keyCode: 53, modifiers: .control))
    #expect(KeyMap.action(keyCode: 53, modifiers: []) == .key("escape"))
  }
}
