import SwiftUI
import Testing

@testable import AtlasUI

@Suite("CommandMenu")
struct CommandMenuTests {
  @Test("an item's shortcut is drawn with Command and the modifiers it names, and none with no shortcut")
  func anItemsShortcutHoldsCommandAndItsModifiers() {
    #expect(CommandMenu.shortcut(ViewerMenuShortcut("r")) == KeyboardShortcut("r", modifiers: .command))
    #expect(
      CommandMenu.shortcut(ViewerMenuShortcut("0", option: true))
        == KeyboardShortcut("0", modifiers: [.command, .option])
    )
    #expect(CommandMenu.shortcut(nil) == nil)
  }

  @Test("a fact is a line that cannot be chosen, and a submenu's items are its sections' in order")
  func aFactCannotBeChosenAndASubmenuListsItsSections() {
    let fact = ViewerMenuItem.info("Folder: understudy")
    #expect(fact.isInfo && !fact.isEnabled && fact.command == nil)
    let keys = ViewerMenuItem(
      "Send Key",
      submenu: [
        ViewerMenuSection([ViewerMenuItem("Spotlight", command: .sendKey(index: 0))], title: "Mac Shortcuts"),
        ViewerMenuSection([ViewerMenuItem("Escape", command: .sendKey(index: 1))], title: "App Shortcuts"),
      ]
    )
    #expect(keys.children.map(\.title) == ["Spotlight", "Escape"])
    #expect(ViewerMenuItem("Picture Updates", children: [fact]).submenu.count == 1, "children make one section")
  }
}
