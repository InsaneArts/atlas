import AtlasCommands
import AtlasCommandsTesting
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("Settings Cursor connections")
@MainActor
struct SettingsCursorTests {
  @Test("Cursor registration says configured and its changes name the correct reload")
  func registrationAndReload() async throws {
    let status = #"{"ok":true,"agents":[{"agent":"cursor","installed":true,"connected":true}]}"#
    let reply = #"{"ok":true,"agents":[{"agent":"cursor","ok":true,"restart":true}]}"#
    let commands = ScriptedCommands(["agents status": status, "agents remove": reply])
    let controller = SettingsController.scripted(commands: commands)
    await controller.loadAgents()
    let row = try #require(controller.state.agents.agents.first { $0.id == "cursor" })
    #expect(row.title == "Cursor")
    #expect(row.connection == .note("Configured"))
    #expect(row.canDisconnect)
    await controller.disconnect("cursor")
    #expect(commands.ran.contains(["agents", "remove", "--cursor"]))
    #expect(controller.state.agents.notice?.text == Words.agentRestart(.cursor))
    #expect(controller.state.agents.notice?.text.contains("Claude") == false)
  }

  @Test("Cursor shows an absent host or a registration error without claiming a connection", arguments: [false, true])
  func unavailableRegistration(hasError: Bool) async throws {
    let status =
      hasError
      ? #"{"ok":true,"agents":[{"agent":"cursor","installed":true,"error":"Cursor's Atlas files changed"}]}"#
      : #"{"ok":true,"agents":[{"agent":"cursor","installed":false,"connected":false}]}"#
    let commands = ScriptedCommands(["agents status": status])
    let controller = SettingsController.scripted(commands: commands)
    await controller.loadAgents()
    let row = try #require(controller.state.agents.agents.first { $0.id == "cursor" })
    #expect(row.connection == (hasError ? .connect(isEnabled: true) : .missing))
    #expect(!row.canDisconnect)
    #expect((row.problem?.contains("files changed") ?? false) == hasError)
  }
}
