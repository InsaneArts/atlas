import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import Testing

@testable import AtlasApp

@Suite("Setup Cursor connections")
struct OnboardingCursorTests {
  @Test("Cursor setup keeps its reload instruction until Continue without reinstalling")
  func reloadInstruction() async throws {
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.finishedMac,
      "agents status": #"{"ok":true,"agents":[{"agent":"cursor","installed":true,"connected":false}]}"#,
      "agents install": #"{"ok":true,"agents":[{"agent":"cursor","ok":true,"restart":true}]}"#,
    ])
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      try await settle("the agents were never read") { controller.snapshot.agentsLoaded }
      await controller.connectAgents(controller.agentsSwitchedOn)
      #expect(controller.snapshot.step == .agents)
      let row = try #require(OnboardingPresenter.agents(controller.snapshot).agents.first { $0.id == "cursor" })
      #expect(row.detail == Words.agentRestart(.cursor))
      await controller.connectAgents(controller.agentsSwitchedOn)
      #expect(controller.snapshot.step == .done)
      #expect(commands.ran.count { $0 == ["agents", "install", "--cursor"] } == 1)
    }
  }

  @Test("an existing Cursor registration says configured without a new install")
  func existingRegistration() async throws {
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.finishedMac,
      "agents status": #"{"ok":true,"agents":[{"agent":"cursor","installed":true,"connected":true}]}"#,
    ])
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      try await settle("the agents were never read") { controller.snapshot.agentsLoaded }
      let row = try #require(OnboardingPresenter.agents(controller.snapshot).agents.first { $0.id == "cursor" })
      #expect(row.detail == "Test Mac apps from Cursor · Configured")
      #expect(row.isOn)
      #expect(!row.isEnabled)
      await controller.connectAgents(controller.agentsSwitchedOn)
      #expect(!commands.ran.contains { $0.starts(with: ["agents", "install"]) })
    }
  }
}
