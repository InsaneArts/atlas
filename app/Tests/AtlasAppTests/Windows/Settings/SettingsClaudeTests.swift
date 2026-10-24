import AtlasCommands
import AtlasCommandsTesting
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("Settings Claude connections")
struct SettingsClaudeTests {
  /// Status with both Claude parts installed and only Claude Code connected.
  private static let partial =
    #"{"ok":true,"agents":[{"agent":"claude","installed":true,"connected":true},{"agent":"claude-app","installed":true,"connected":false},{"agent":"codex","installed":false}]}"#

  @Test("one Claude row names both parts and can finish or disconnect a partial connection")
  func partialConnection() async throws {
    let commands = ScriptedCommands(["agents status": Self.partial, "agents install": #"{"ok":true}"#])
    let controller = SettingsController.scripted(commands: commands)
    await controller.loadAgents()
    let rows = controller.state.agents.agents
    #expect(rows.map(\.id) == ["claude", "codex"])
    let claude = try #require(rows.first)
    #expect(claude.title == "Claude")
    #expect(claude.detail == "Claude Code and the Claude app")
    #expect(claude.connection == .connect(isEnabled: true))
    #expect(claude.canDisconnect)
    #expect(claude.icon == .app(AgentLogos.claude))
    await controller.connect("claude")
    #expect(commands.ran.contains(["agents", "install", "--claude", "--app"]))
  }

  @Test("Disconnect removes both Claude connections")
  func disconnectsBothParts() async {
    let commands = ScriptedCommands(["agents status": Self.partial, "agents remove": #"{"ok":true}"#])
    let controller = SettingsController.scripted(commands: commands)
    await controller.loadAgents()
    await controller.disconnect("claude")
    #expect(commands.ran.contains(["agents", "remove", "--claude", "--app"]))
  }

  @Test("neither Claude part installed shows Not installed and sends no Connect command")
  func neitherPartIsInstalled() async throws {
    let commands = ScriptedCommands([
      "agents status":
        #"{"ok":true,"agents":[{"agent":"claude","installed":false},{"agent":"claude-app","installed":false}]}"#
    ])
    let controller = SettingsController.scripted(commands: commands)
    await controller.loadAgents()
    let claude = try #require(controller.state.agents.agents.first)
    #expect(claude.detail == "Not installed")
    #expect(claude.connection == .missing)
    await controller.connect("claude")
    #expect(!commands.ran.contains { $0.starts(with: ["agents", "install"]) })
  }

  @Test("failures in both Claude parts are kept in the one row")
  func bothFailuresAreShown() async throws {
    let commands = ScriptedCommands([
      "agents status": Self.partial,
      "agents install":
        #"{"ok":false,"agents":[{"agent":"claude","ok":false,"error":"plugin failed"},{"agent":"claude-app","ok":false,"error":"settings file is not valid JSON"}]}"#,
    ])
    let controller = SettingsController.scripted(commands: commands)
    await controller.loadAgents()
    await controller.connect("claude")
    let claude = try #require(controller.state.agents.agents.first)
    #expect(claude.problem?.contains("Claude Code") == true)
    #expect(claude.problem?.contains("The Claude app") == true)
    #expect(claude.showsFile)
  }

  @Test("one installed Claude part is named and connected alone", arguments: ["claude", "claude-app"])
  func oneInstalledPart(part: String) async throws {
    let commands = ScriptedCommands([
      "agents status": """
      {"ok":true,"agents":[{"agent":"\(part)","installed":true,"connected":false}]}
      """,
      "agents install": #"{"ok":true}"#,
    ])
    let controller = SettingsController.scripted(commands: commands)
    await controller.loadAgents()
    let claude = try #require(controller.state.agents.agents.first)
    #expect(claude.title == "Claude")
    #expect(claude.detail == (part == "claude" ? "Claude Code" : "The Claude app"))
    await controller.connect("claude")
    #expect(commands.ran.contains(["agents", "install", part == "claude" ? "--claude" : "--app"]))
  }

  @Test(
    "a failure in either Claude part stays in the single row and names that part",
    arguments: ["claude", "claude-app"]
  )
  func failureNamesThePart(part: String) async throws {
    let commands = ScriptedCommands([
      "agents status": Self.partial,
      "agents install": """
      {"ok":false,"agents":[{"agent":"\(part)","ok":false,"error":"could not write settings"}]}
      """,
    ])
    let controller = SettingsController.scripted(commands: commands)
    await controller.loadAgents()
    await controller.connect("claude")
    let claude = try #require(controller.state.agents.agents.first)
    #expect(claude.problem?.contains(AgentsStatus.partTitle(part)) == true)
    #expect(claude.connection != .connected)
  }
}
