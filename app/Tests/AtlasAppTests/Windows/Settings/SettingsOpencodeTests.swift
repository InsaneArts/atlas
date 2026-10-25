import AtlasCommands
import AtlasCommandsTesting
import Testing

@testable import AtlasApp

@Suite("Settings opencode connections")
@MainActor
struct SettingsOpencodeTests {
  @Test(
    "connecting or disconnecting opencode names its restart without a Claude instruction",
    arguments: [false, true]
  )
  func restartNotice(disconnect: Bool) async throws {
    let status = #"{"ok":true,"agents":[{"agent":"opencode","installed":true,"connected":true}]}"#
    let reply = #"{"ok":true,"agents":[{"agent":"opencode","ok":true,"restart":true}]}"#
    let commands = ScriptedCommands(["agents status": status, "agents install": reply, "agents remove": reply])
    let controller = SettingsController.scripted(commands: commands)
    await controller.loadAgents()
    if disconnect { await controller.disconnect("opencode") } else { await controller.connect("opencode") }
    #expect(controller.state.agents.notice?.text == "Restart opencode for this change to take effect.")
    #expect(commands.ran.contains(["agents", disconnect ? "remove" : "install", "--opencode"]))
  }
}
