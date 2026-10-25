import AppKit
import AtlasCommandsTesting
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("Settings interactions", .serialized)
struct SettingsInteractionTests {
  @Test("returning from either detail restores its disclosure and Return opens only that detail")
  func returningRestoresTheDisclosure() async throws {
    for detail in [SettingsDestination.diagnostics, .projects] {
      let commands = ScriptedCommands(["agents status": #"{"ok":true,"agents":[],"projects":[]}"#])
      let controller = SettingsController.scripted(commands: commands)
      controller.send(.navigate(.root(detail.root)))
      let keyboard = SettingsSearchKeyboard(controller)
      defer { keyboard.close() }
      let editor = try await keyboard.focus()
      controller.send(.navigate(detail))
      try await keyboard.waitForFocus {
        keyboard.window.firstResponder is NSView && keyboard.window.firstResponder !== editor
      }
      controller.send(.back)
      #expect(controller.state.reveal?.target == .disclosure(detail))
      try await keyboard.press("\r", code: 36)
      #expect(controller.state.location == detail)
      #expect(controller.state.alert == nil)
      #expect(commands.ran.allSatisfy { $0 == ["agents", "status"] })
    }
  }

  @Test("search keeps field focus on clear and disabled history cannot move its underlying page")
  func returningFromSearchRestoresFocus() async throws {
    let controller = SettingsController.scripted(commands: ScriptedCommands([:]))
    let keyboard = SettingsSearchKeyboard(controller)
    defer { keyboard.close() }
    let editor = try await keyboard.focus()
    controller.send(.search("helper"))
    try await keyboard.press("h", code: 4)
    let history = controller.session.navigation
    controller.send(.back)
    controller.send(.forward)
    #expect(controller.session.navigation == history)
    try await keyboard.press("\u{1b}", code: 53)
    #expect(!controller.state.search.isActive)
    #expect(controller.state.location == .root(.general))
    #expect(keyboard.window.firstResponder === editor)
  }

  @Test("an alert contains focus and blocks background history search and helper actions")
  func anAlertContainsInteraction() async throws {
    let commands = ScriptedCommands([:])
    let controller = SettingsController.scripted(commands: commands)
    controller.send(.navigate(.root(.helper)))
    let keyboard = SettingsSearchKeyboard(controller)
    defer { keyboard.close() }
    let editor = try await keyboard.focus()
    controller.send(.askRecover)
    keyboard.window.contentView?.layoutSubtreeIfNeeded()
    let before = controller.session.navigation
    controller.send(.back)
    controller.send(.search("helper"))
    try await keyboard.press("\t", code: 48)
    #expect(keyboard.window.firstResponder !== editor)
    #expect(controller.session.navigation == before && !controller.state.search.isActive)
    #expect(commands.ran.isEmpty)
    try await keyboard.press("\u{1b}", code: 53)
    #expect(controller.state.alert == nil && commands.ran.isEmpty)
  }

}
