import AtlasCommands
import AtlasCommandsTesting
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("Settings Claude connections")
struct SettingsClaudeTests {
  @Test(
    "Connect and Disconnect show the Claude restart line only when requested",
    arguments: ["true", "false", "missing"],
    [false, true]
  )
  func restartNotice(restart: String, disconnect: Bool) async throws {
    let field = restart == "missing" ? "" : ",\"restart\":\(restart)"
    let commands = ScriptedCommands([
      "agents status":
        "{\"ok\":true,\"agents\":[{\"agent\":\"claude-app\",\"installed\":true,\"connected\":\(disconnect)}]}",
      disconnect ? "agents remove" : "agents install":
        "{\"ok\":true,\"agents\":[{\"agent\":\"claude-app\",\"ok\":true\(field)}]}",
    ])
    let controller = SettingsController.scripted(commands: commands)
    await controller.loadAgents()
    #expect(controller.state.agents.notice == nil)
    commands.answer(
      "agents status",
      "{\"ok\":true,\"agents\":[{\"agent\":\"claude-app\",\"installed\":true,\"connected\":\(!disconnect)}]}"
    )
    if disconnect {
      await controller.disconnect("claude")
    } else {
      await controller.connect("claude")
    }
    let row = try #require(controller.state.agents.agents.first)
    #expect(row.connection == (disconnect ? .connect(isEnabled: true) : .connected))
    #expect(
      controller.state.agents.notice?.text
        == (restart == "true" ? "Quit and reopen the Claude app for this to take effect." : nil)
    )
    #expect(controller.state.agents.notice?.kind == (restart == "true" ? .quiet : nil))
  }

  /// Status with both Claude parts installed and only Claude Code connected.
  private static let partial =
    #"{"ok":true,"agents":[{"agent":"claude","installed":true,"connected":true},{"agent":"claude-app","installed":true,"connected":false},{"agent":"codex","installed":false}]}"#

  @Test("one Claude row names both parts and can finish or disconnect a partial connection")
  func partialConnection() async throws {
    let commands = ScriptedCommands(["agents status": Self.partial, "agents install": #"{"ok":true}"#])
    let controller = SettingsController.scripted(commands: commands)
    await controller.loadAgents()
    let rows = controller.state.agents.agents
    #expect(rows.map(\.id) == ["claude", "codex", "opencode", "pi", "cursor"])
    let claude = try #require(rows.first)
    #expect(claude.title == "Claude")
    #expect(claude.detail == "Claude Code tools and a live Mac view in the app")
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

  @Test("a failed Connect stays when its own status reread reports every Claude part connected")
  func connectFailureStaysAfterOwnStatusRead() async throws {
    let commands = ScriptedCommands([
      "agents status": Self.partial,
      "agents install":
        #"{"ok":false,"agents":[{"agent":"claude","ok":false,"error":"plugin failed"},{"agent":"claude-app","ok":true}]}"#,
    ])
    let controller = SettingsController.scripted(commands: commands)
    await controller.loadAgents()
    commands.answer(
      "agents status",
      #"{"ok":true,"agents":[{"agent":"claude","installed":true,"connected":true},{"agent":"claude-app","installed":true,"connected":true}]}"#
    )
    await controller.connect("claude")
    let failed = try #require(controller.state.agents.agents.first)
    #expect(failed.connection == .connected)
    #expect(failed.problem?.contains("plugin failed") == true)
    #expect(controller.session.connectFailures["claude"] != nil)
  }

  @Test("a failed Connect clears on a later refresh after its own reread reports connected")
  func connectFailureClearsAfterLaterRefresh() async throws {
    let commands = ScriptedCommands([
      "agents status": Self.partial,
      "agents install":
        #"{"ok":false,"agents":[{"agent":"claude","ok":false,"error":"plugin failed"},{"agent":"claude-app","ok":true}]}"#,
    ])
    let controller = SettingsController.scripted(commands: commands)
    await controller.loadAgents()
    commands.answer(
      "agents status",
      #"{"ok":true,"agents":[{"agent":"claude","installed":true,"connected":true},{"agent":"claude-app","installed":true,"connected":true}]}"#
    )
    await controller.connect("claude")
    let failed = try #require(controller.state.agents.agents.first)
    #expect(failed.problem?.contains("plugin failed") == true)
    await controller.loadAgents()
    let refreshed = try #require(controller.state.agents.agents.first)
    #expect(refreshed.connection == .connected)
    #expect(refreshed.problem == nil)
    #expect(controller.session.connectFailures["claude"] == nil)
  }

  @Test("a failed Disconnect stays when an installed Claude part has unknown connection status")
  func disconnectFailureStaysAfterUnknownStatus() async throws {
    let commands = ScriptedCommands([
      "agents status": Self.partial,
      "agents remove":
        #"{"ok":false,"agents":[{"agent":"claude","ok":false,"error":"could not remove plugin"}]}"#,
    ])
    let controller = SettingsController.scripted(commands: commands)
    await controller.loadAgents()
    commands.answer(
      "agents status",
      #"{"ok":true,"agents":[{"agent":"claude","installed":true,"connected":null},{"agent":"claude-app","installed":true,"connected":false}]}"#
    )
    await controller.disconnect("claude")
    let failed = try #require(controller.state.agents.agents.first)
    #expect(failed.problem?.contains("Couldn't disconnect") == true)
    #expect(failed.problem?.contains("could not remove plugin") == true)
    #expect(controller.session.connectFailures["claude"]?.isDisconnect == true)
    await controller.loadAgents()
    #expect(controller.state.agents.agents.first?.problem == failed.problem)
    #expect(controller.session.connectFailures["claude"]?.isDisconnect == true)
  }

  @Test("a failed Connect clears after connecting elsewhere and refreshing Settings")
  func connectFailureClearsAfterRefresh() async throws {
    let commands = ScriptedCommands([
      "agents status": Self.partial,
      "agents install":
        #"{"ok":false,"agents":[{"agent":"claude-app","ok":false,"error":"settings file is not valid JSON"}]}"#,
    ])
    let controller = SettingsController.scripted(commands: commands)
    await controller.loadAgents()
    await controller.connect("claude")
    let failed = try #require(controller.state.agents.agents.first)
    #expect(failed.problem?.contains("Couldn't connect") == true)
    #expect(failed.showsFile)
    controller.close()
    commands.answer(
      "agents status",
      #"{"ok":true,"agents":[{"agent":"claude","installed":true,"connected":true},{"agent":"claude-app","installed":true,"connected":true}]}"#
    )
    controller.cameForward()
    await controller.agentsReading?.value
    let refreshed = try #require(controller.state.agents.agents.first)
    #expect(refreshed.connection == .connected)
    #expect(refreshed.problem == nil)
    #expect(!refreshed.showsFile)
    #expect(controller.session.connectFailures["claude"] == nil)
  }

  @Test("a failed Disconnect stays until a refresh confirms every Claude part is disconnected")
  func disconnectFailureClearsAfterRefresh() async throws {
    let commands = ScriptedCommands([
      "agents status":
        #"{"ok":true,"agents":[{"agent":"claude","installed":true,"connected":true},{"agent":"claude-app","installed":true,"connected":true}]}"#,
      "agents remove":
        #"{"ok":false,"agents":[{"agent":"claude-app","ok":false,"error":"settings file is not valid JSON"}]}"#,
    ])
    let controller = SettingsController.scripted(commands: commands)
    await controller.loadAgents()
    await controller.disconnect("claude")
    let failed = try #require(controller.state.agents.agents.first)
    #expect(failed.connection == .connected)
    #expect(failed.problem?.contains("Couldn't disconnect") == true)
    #expect(failed.showsFile)
    commands.answer("agents status", Self.partial)
    await controller.loadAgents()
    #expect(controller.state.agents.agents.first?.problem == failed.problem)
    commands.answer(
      "agents status",
      #"{"ok":true,"agents":[{"agent":"claude","installed":true,"connected":false},{"agent":"claude-app","installed":true,"connected":false}]}"#
    )
    await controller.loadAgents()
    let refreshed = try #require(controller.state.agents.agents.first)
    #expect(refreshed.connection == .connect(isEnabled: true))
    #expect(refreshed.problem == nil)
    #expect(!refreshed.showsFile)
    #expect(controller.session.connectFailures["claude"] == nil)
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
    #expect(claude.detail == (part == "claude" ? "Test Mac apps from Claude Code" : "Live Mac view in the Claude app"))
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
