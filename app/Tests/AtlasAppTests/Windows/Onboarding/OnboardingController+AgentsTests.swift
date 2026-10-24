import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasOnboarding
import Testing

@testable import AtlasApp

@Suite("OnboardingController agents")
struct OnboardingControllerAgentsTests {
  /// `atlas agents status` finding Claude Code installed and not connected.
  private static let claudeFound =
    #"{"ok": true, "agents": [{"agent": "claude", "installed": true, "connected": false}]}"#

  @Test("closing setup while a command runs lets it settle and moves on to nothing")
  func closingWhileACommandRunsMovesOnToNothing() async throws {
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.finishedMac, "agents status": Self.claudeFound,
      "agents install": #"{"ok": true}"#,
    ])
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      try await settle("the agents were never read") { controller.snapshot.agentsLoaded }
      // The window closes while the agents are installed.
      commands.whileRunning { arguments in
        if arguments.starts(with: ["agents", "install"]) { await controller.close() }
      }
      await controller.connectAgents()
      #expect(commands.ran.contains(Action.installAgents([.claude]).arguments), "the running command settled")
      #expect(!controller.snapshot.choices.agentsDone, "the closed window moved on to nothing")
    }
  }

  @Test("agents read once the window has closed change nothing")
  func lateAgentsAreDropped() async throws {
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.finishedMac, "agents status": Self.claudeFound,
    ])
    let controller = OnboardingController.scripted(commands)
    commands.whileRunning { arguments in
      if arguments == Action.agentsStatus.arguments { await controller.close() }
    }
    try await controller.whileOpen { running in
      try await settle("the agents were never asked for") { commands.ran.contains(Action.agentsStatus.arguments) }
      // run()'s own end is the sentinel: it ends once the agents reading under way has done all it ever will.
      try await running.value("the agents reading never ended")
      #expect(!controller.snapshot.agentsLoaded, "the closed window took the agents' reply")
    }
  }

  @Test(
    "a failed agents reading keeps the agents listed unchecked, lets Continue skip the step, and is asked again later"
  )
  func aFailedAgentsReadingIsAskedAgainLater() async throws {
    let commands = ScriptedCommands(["onboarding status": OnboardingReplies.finishedMac])
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      try await settle("the failed agents reading was never shown") { controller.snapshot.agentsFailed }
      #expect(controller.snapshot.problem == OnboardingWords.agentsCheckFailed)
      #expect(controller.snapshot.agents.map(\.id) == AgentID.allCases, "the agents stay listed")
      #expect(controller.state.footer.next?.isEnabled == true, "Continue skips the step")
      controller.refresh()
      try await settle("the next reading never ran") {
        commands.ran.count { $0 == ["onboarding", "status"] } >= 2
      }
      #expect(
        commands.ran.count { $0 == ["agents", "status"] } == 1,
        "not asked again with the very next reading"
      )
      commands.answer("agents status", Self.claudeFound)
      try await settle("the agents were never asked for again") {
        controller.refresh()
        return controller.snapshot.agentsLoaded
      }
      #expect(controller.snapshot.agents.first { $0.id == .claude }?.found == true)
      #expect(controller.snapshot.problem == nil)
    }
  }

  @Test("agents that cannot be connected are each named with why, and switching them off moves on without the note")
  func agentsThatCannotBeConnectedAreNamed() async throws {
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.finishedMac,
      "agents status": """
      {"ok": true, "agents": [{"agent": "claude", "installed": true, "connected": false},
                              {"agent": "codex", "installed": true, "connected": false}]}
      """,
      "agents install": """
      {"ok": false, "agents": [{"agent": "claude", "ok": true, "connected": true},
                               {"agent": "codex", "ok": false, "error": "codex plugin install failed"}]}
      """,
    ])
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      try await settle("the agents were never read") { controller.snapshot.agentsLoaded }
      await controller.connectAgents()
      #expect(commands.ran.contains(Action.installAgents([.claude, .codex]).arguments))
      #expect(controller.snapshot.problem == OnboardingWords.notConnected(.codex, "codex plugin install failed"))
      #expect(controller.snapshot.agents.first { $0.id == .claude }?.connected == true)
      #expect(!controller.snapshot.choices.agentsDone)
      // Codex switched off, Continue moves on without the failed install's note.
      controller.send(.setAgent(id: "codex", on: false))
      await controller.connectAgents()
      #expect(commands.ran.count { $0.starts(with: ["agents", "install"]) } == 1)
      #expect(controller.snapshot.problem == nil)
      #expect(controller.snapshot.choices.agentsDone)
    }
  }

  @Test("the agents step lists every agent with what atlas agents status reports")
  func theAgentsAreLoaded() async throws {
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.finishedMac, "agents status": Self.claudeFound,
    ])
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      try await settle("the agents were never read") { controller.snapshot.agentsLoaded }
      #expect(controller.snapshot.agents.map(\.id) == AgentID.allCases)
      #expect(controller.snapshot.agents.first?.on == true, "a found agent is switched on")
    }
  }

  @Test("an agents install Settings started holds the setup window's Continue, and the two never install at once")
  func installsFromBothWindowsNeverOverlap() async throws {
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.finishedMac, "agents status": Self.claudeFound,
      "agents install": #"{"ok": true}"#,
    ])
    let gate = ReleaseSignal()
    commands.whileRunning { arguments in
      if arguments.prefix(2) == ["agents", "install"] { await gate.wait() }
    }
    let installs = AgentInstalls(commands: commands)
    let onboarding = OnboardingController.scripted(commands, installs: installs)
    try await onboarding.whileOpen { _ in
      try await onboarding.untilLoaded()
      try await settle("the agents were never read") { onboarding.snapshot.agentsLoaded }
      let settings = SettingsController.scripted(commands: commands, installs: installs)
      let connecting = Task { await settings.connect("codex") }
      try await settle("Settings' Connect never started") { installs.isBusy }
      #expect(onboarding.state.footer.next?.isEnabled == false, "Continue waits for the other window's install")
      await onboarding.connectAgents()
      #expect(onboarding.snapshot.problem == HelperWords.waitForAction)
      gate.open()
      await connecting.value
      let installed = commands.ran.filter { $0.prefix(2) == ["agents", "install"] }
      #expect(installed == [Action.installAgents([.codex]).arguments])
    }
  }

  @Test("Continue after a skipped agents check clears its note and moves on to Done, connecting nothing")
  func continueAfterASkippedCheckClearsItsNote() async throws {
    let commands = ScriptedCommands(["onboarding status": OnboardingReplies.finishedMac])
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      try await settle("the failed agents reading was never shown") { controller.snapshot.agentsFailed }
      #expect(controller.snapshot.problem == OnboardingWords.agentsCheckFailed)
      await controller.connectAgents()
      #expect(!commands.ran.contains { $0.starts(with: ["agents", "install"]) })
      #expect(controller.snapshot.problem == nil)
      #expect(controller.snapshot.choices.agentsDone)
    }
  }

  @Test("Continue connects the chosen agents and moves on to Done")
  func continueConnectsTheAgents() async throws {
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.finishedMac, "agents status": Self.claudeFound,
      "agents install": #"{"ok": true}"#,
    ])
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      try await settle("the agents were never read") { controller.snapshot.agentsLoaded }
      await controller.connectAgents()
      #expect(commands.ran.contains(Action.installAgents([.claude]).arguments))
      #expect(controller.snapshot.agents.first?.connected == true)
      #expect(controller.snapshot.problem == nil)
      #expect(controller.snapshot.choices.agentsDone, "the window moves on to Done")
    }
  }

  @Test("a failed install connects no agent and stays on the agents, saying why")
  func aFailedInstallStops() async throws {
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.finishedMac, "agents status": Self.claudeFound,
      "agents install": #"{"ok": false, "error": "npm is missing"}"#,
    ])
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      try await settle("the agents were never read") { controller.snapshot.agentsLoaded }
      await controller.connectAgents()
      #expect(commands.ran.contains(Action.installAgents([.claude]).arguments))
      #expect(controller.snapshot.agents.allSatisfy { !$0.connected })
      #expect(!controller.snapshot.choices.agentsDone)
      #expect(controller.snapshot.problem == "npm is missing.")
    }
  }

  @Test("an event sent while a command runs waits its turn, then runs")
  func anEventWaitsItsTurn() async throws {
    let gate = ReleaseSignal()
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.finishedMac, "agents status": Self.claudeFound,
      "agents install": #"{"ok": true}"#,
    ])
    commands.whileRunning { arguments in
      if arguments == Action.agentsStatus.arguments { await gate.wait() }
    }
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      try await settle("the agents were never asked for") { commands.ran.contains(Action.agentsStatus.arguments) }
      controller.send(.connectAgents)
      await afterQueuedWork()
      #expect(!commands.ran.contains { $0.starts(with: ["agents", "install"]) }, "Continue waits for the reading")
      #expect(!controller.snapshot.choices.agentsDone, "nor moves on without the agents it finds")
      gate.open()
      try await settle("Continue never ran") { controller.snapshot.choices.agentsDone }
      #expect(commands.ran.contains(Action.installAgents([.claude]).arguments), "it connects what the reading found")
    }
  }
}
