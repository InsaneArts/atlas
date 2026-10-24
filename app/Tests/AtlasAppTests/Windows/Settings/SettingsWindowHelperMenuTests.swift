import AppKit
import AtlasCommandsTesting
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("SettingsWindow Helper menu")
struct SettingsWindowHelperMenuTests {
  @Test("with Settings in front, the Helper menu's Recover…, Switch and Log Out follow its Helper pane's buttons")
  func settingsAnswersTheHelperMenu() throws {
    let controller = SettingsController.scripted(commands: ScriptedCommands([:]))
    let window = SettingsWindow(controller: controller, closed: { _ in })
    controller.send(.navigate(.diagnostics))
    let helper = MainMenu.helper()
    let switchItem = try #require(helper.items.first { $0.action == #selector(HelperMenuActions.askSwitch(_:)) })
    let restart = try #require(helper.items.first { $0.action == #selector(HelperMenuActions.askRestart(_:)) })
    let buttons = controller.state.helper.buttons
    #expect(window.validateMenuItem(switchItem) == buttons.canSwitch)
    #expect(switchItem.title == "Switch to “\(controller.state.helper.account.name)”…")
    #expect(!window.validateMenuItem(restart), "Settings offers no restart")
    window.askSwitch(nil)
    #expect(controller.state.location == .root(.helper), "it asks on the Helper page")
    #expect(controller.state.alert != nil)
  }
}
