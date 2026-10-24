import AppKit
import Testing

@testable import AtlasApp

@Suite("MainMenu")
struct MainMenuTests {
  /// Answers the app's own items in a test.
  final class Answerer: NSObject {
    /// Stands in for any item's action.
    @objc func act(_ sender: Any?) {}
  }

  /// The menus with every app item given, as the running app builds them.
  ///
  /// - Parameter answerer: Who answers the app's own items.
  /// - Returns: The menus, by their titles.
  static func menus(_ answerer: Answerer) -> [String: NSMenu] {
    let command: MainMenu.Command = (answerer, #selector(Answerer.act(_:)))
    let menu = MainMenu.make(
      quit: command,
      settings: command,
      checkForUpdates: command,
      gettingStarted: command,
      openViewer: command,
      copyDiagnostics: command,
      viewer: true
    )
    return Dictionary(uniqueKeysWithValues: menu.items.compactMap(\.submenu).map { ($0.title, $0) })
  }

  /// A menu's items as they read, a separator as "—".
  ///
  /// - Parameter menu: The menu.
  /// - Returns: The titles.
  static func titles(_ menu: NSMenu?) -> [String] {
    (menu?.items ?? [])
      .map { $0.isSeparatorItem ? "—" : $0.title }
  }

  @Test("Close ⌘W is in the menus with the main window and with the setup window on its own", arguments: [true, false])
  func closeIsAlwaysOffered(viewer: Bool) {
    let items = MainMenu.make(viewer: viewer).items
      .compactMap(\.submenu)
      .flatMap(\.items)
    let close = items.filter { $0.action == #selector(NSWindow.performClose(_:)) }
    #expect(close.count == 1)
    #expect(close.first?.keyEquivalent == "w" && close.first?.keyEquivalentModifierMask == .command)
  }

  @Test("the Atlas and Window menus carry every Mac app's standard items, in their order")
  func theStandardItemsAreThere() {
    let answerer = Answerer()
    let menus = Self.menus(answerer)
    #expect(
      Self.titles(menus["Atlas"]) == [
        "About Atlas", "—", "Settings…", "Check for Updates…", "—", "Services", "—", "Hide Atlas", "Hide Others",
        "Show All", "—", "Quit Atlas",
      ]
    )
    #expect(Self.titles(menus["Window"]) == ["Minimize", "Zoom", "—", "Bring All to Front"])
    #expect(Self.titles(menus["Help"]) == ["Getting Started…", "Copy Diagnostics"])
  }

  @Test("File opens with Open Atlas ⌘O, the way back to the main window, and Edit finds with ⌘F")
  func fileLeadsBackToTheMainWindow() throws {
    let answerer = Answerer()
    let menus = Self.menus(answerer)
    #expect(Self.titles(menus["File"]) == ["Open Atlas", "—", "Save Picture…", "—", "Close"])
    let open = try #require(menus["File"]?.items.first)
    #expect(open.keyEquivalent == "o" && open.target === answerer)
    let find = try #require(menus["Edit"]?.items.first { $0.action == #selector(ViewerMenuActions.findInTab(_:)) })
    #expect(find.title == "Find…" && find.keyEquivalent == "f")
  }

  @Test("the Helper menu offers Recover…, with Restart Helper… only while Option is held, then Switch and Log Out")
  func theHelperMenuHidesRestartBehindOption() throws {
    let helper = try #require(Self.menus(Answerer())["Helper"])
    #expect(
      Self.titles(helper) == [
        "Take Control", "Stop Agent", "—", "Recover…", "Restart Helper…", "—", "Switch to the Helper…",
        "Log Out the Helper…",
      ]
    )
    let restart = try #require(helper.items.first { $0.action == #selector(HelperMenuActions.askRestart(_:)) })
    #expect(restart.isAlternate && restart.keyEquivalentModifierMask == .option)
  }

  @Test("Switch and Log Out name the helper once a window knows it")
  func helperItemsNameTheHelper() {
    #expect(MenuWords.switchTo("Atlas") == "Switch to “Atlas”…")
    #expect(MenuWords.logOut("Atlas") == "Log Out “Atlas”…")
    #expect(MenuWords.switchTo(nil) == "Switch to the Helper…")
  }
}
