import AppKit
import Testing

@testable import AtlasApp

@Suite("MainMenu")
struct MainMenuTests {
  @Test("Close ⌘W is in the menus with the main window and with the setup window on its own", arguments: [true, false])
  func closeIsAlwaysOffered(viewer: Bool) {
    let items = MainMenu.make(viewer: viewer).items
      .compactMap(\.submenu)
      .flatMap(\.items)
    let close = items.filter { $0.action == #selector(NSWindow.performClose(_:)) }
    #expect(close.count == 1)
    #expect(close.first?.keyEquivalent == "w" && close.first?.keyEquivalentModifierMask == .command)
  }
}
