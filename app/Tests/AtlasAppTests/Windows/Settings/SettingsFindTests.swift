import AppKit
import AtlasCommandsTesting
import Testing

@testable import AtlasApp

@Suite("Settings Find", .serialized, AppKitState())
struct SettingsFindTests {
  @Test("the one menu Find action focuses Settings search and waits underneath an alert")
  func settingsAnswersTheExistingFindAction() async throws {
    let commands = ScriptedCommands([:])
    let controller = SettingsController.scripted(commands: commands)
    let keyboard = SettingsSearchKeyboard(controller)
    defer { keyboard.close() }
    let window = keyboard.window
    let menu = MainMenu.make(viewer: true)
    let shortcuts = menu.items
      .flatMap { $0.submenu?.items ?? [] }
      .filter { $0.keyEquivalent == "f" && $0.keyEquivalentModifierMask == .command }
    let find = try #require(shortcuts.first)
    #expect(shortcuts.count == 1 && find.target == nil)
    #expect(find.action == #selector(FindMenuActions.findInTab(_:)))
    #expect(window.validateMenuItem(find))
    let editor = try await keyboard.focus()
    #expect(window.firstResponder === editor)
    #expect(controller.state.search.focusRequests == 1)
    controller.send(.askRecover)
    #expect(!window.validateMenuItem(find))
    try await keyboard.press("f", code: 3, modifiers: .command)
    #expect(controller.state.search.focusRequests == 1)
    #expect(commands.ran.isEmpty)
  }
}
