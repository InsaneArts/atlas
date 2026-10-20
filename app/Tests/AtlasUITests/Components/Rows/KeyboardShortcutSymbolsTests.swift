import SwiftUI
import Testing

@testable import AtlasUI

@Suite("KeyboardShortcut+Symbols")
struct KeyboardShortcutSymbolsTests {
  @Test("a shortcut reads as a menu writes it: ⌃⌥⇧⌘ in that order, then its key in capitals")
  func readsAsAMenuWritesIt() {
    #expect(KeyboardShortcut("q", modifiers: .command).symbols == "⌘Q")
    #expect(KeyboardShortcut("p", modifiers: [.command, .shift, .option, .control]).symbols == "⌃⌥⇧⌘P")
  }
}
