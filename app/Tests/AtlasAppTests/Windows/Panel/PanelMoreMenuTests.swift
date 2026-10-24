import AppKit
import Testing

@testable import AtlasApp

@Suite("PanelMoreMenu")
struct PanelMoreMenuTests {
  /// The menu, each item counting its choices in `chosen`.
  ///
  /// - Parameters:
  ///   - showsOpenAtlas: Whether Open Atlas leads it.
  ///   - chosen: Counts each item's choices, by its title.
  /// - Returns: The menu.
  private static func menu(showsOpenAtlas: Bool, chosen: ChoiceCount = ChoiceCount()) -> NSMenu {
    PanelMoreMenu.menu(
      PanelMoreMenuState(showsOpenAtlas: showsOpenAtlas),
      actions: PanelMoreMenu.Actions(
        openAtlas: { chosen.add(PanelWords.openAtlasItem) },
        openSettings: { chosen.add(MenuWords.settings) },
        gettingStarted: { chosen.add(MenuWords.gettingStarted) },
        quit: { chosen.add(MenuWords.quit) }
      )
    )
  }

  @Test("the panel's ⋯ holds Open Atlas, Settings…, Getting Started… and Quit Atlas, and no helper command")
  func theMenuHoldsWhatAppliesEverywhere() {
    let titles = Self.menu(showsOpenAtlas: true)
      .items
      .map(\.title)
    #expect(titles == [PanelWords.openAtlasItem, MenuWords.settings, MenuWords.gettingStarted, "", MenuWords.quit])
    for helper in [HelperWords.recoverItem, HelperWords.switchItem, HelperWords.logOutItem] {
      #expect(!titles.contains(helper), "\(helper) lives with the helper's own controls")
    }
    #expect(!titles.contains("Open at Login"), "Settings ▸ General keeps it")
  }

  @Test("Open Atlas leaves the menu while the main button already is Open Atlas")
  func openAtlasIsNeverOfferedTwice() {
    let titles = Self.menu(showsOpenAtlas: false)
      .items
      .map(\.title)
    #expect(!titles.contains(PanelWords.openAtlasItem))
    #expect(titles.first == MenuWords.settings)
  }

  @Test("every item is enabled, and each keeps its shortcut: ⌘O, ⌘, and ⌘Q")
  func everyItemWorks() throws {
    let items = Self.menu(showsOpenAtlas: true)
      .items
      .filter { !$0.isSeparatorItem }
    #expect(items.allSatisfy { $0.isEnabled })
    let keys = Dictionary(uniqueKeysWithValues: items.map { ($0.title, $0.keyEquivalent) })
    #expect(keys[PanelWords.openAtlasItem] == "o")
    #expect(keys[MenuWords.settings] == ",")
    #expect(keys[MenuWords.gettingStarted] == "")
    #expect(keys[MenuWords.quit] == "q")
  }

  @Test("the panel's ⋯ Settings… and Getting Started… each run their own action")
  func itemsRunTheirActions() throws {
    let chosen = ChoiceCount()
    let menu = Self.menu(showsOpenAtlas: true, chosen: chosen)
    for title in [MenuWords.settings, MenuWords.gettingStarted] {
      let item = try #require(menu.items.first { $0.title == title })
      // The item's own target and action, as the menu sends them: a test process has no running app to route
      // `performActionForItem` through.
      let target = try #require(item.target as? NSObject)
      let action = try #require(item.action)
      _ = target.perform(action, with: item)
    }
    #expect(chosen.titles == [MenuWords.settings, MenuWords.gettingStarted])
  }

  @Test("the panel answers ⌘O, ⌘, and ⌘Q with Command alone, and lets every other key pass")
  func thePanelAnswersItsShortcuts() {
    #expect(PanelKeys.shortcut(characters: "o", modifiers: .command) == .openAtlas)
    #expect(PanelKeys.shortcut(characters: ",", modifiers: .command) == .openSettings)
    #expect(PanelKeys.shortcut(characters: "q", modifiers: .command) == .quit)
    #expect(PanelKeys.shortcut(characters: ".", modifiers: .command) == .stopOrResume)
    #expect(PanelKeys.shortcut(characters: "P", modifiers: [.command, .shift]) == .pausePicture)
    #expect(PanelKeys.shortcut(characters: "p", modifiers: .command) == nil)
    #expect(PanelKeys.shortcut(characters: ".", modifiers: [.command, .option]) == nil)
    #expect(PanelKeys.shortcut(characters: "q", modifiers: [.command, .shift]) == nil, "⇧⌘Q is the system's")
    #expect(PanelKeys.shortcut(characters: "o", modifiers: []) == nil)
    #expect(PanelKeys.shortcut(characters: "w", modifiers: .command) == nil)
  }
}
