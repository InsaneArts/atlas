import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import Testing

@testable import AtlasApp

@Suite("Setup opencode connections")
struct OnboardingOpencodeTests {
  @Test("setup keeps opencode's restart instruction on its own row until Continue")
  func restartInstruction() async throws {
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.finishedMac,
      "agents status": #"{"ok":true,"agents":[{"agent":"opencode","installed":true,"connected":false}]}"#,
      "agents install": #"{"ok":true,"agents":[{"agent":"opencode","ok":true,"restart":true}]}"#,
    ])
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      try await settle("the agents were never read") { controller.snapshot.agentsLoaded }
      await controller.connectAgents(controller.agentsSwitchedOn)
      #expect(controller.snapshot.step == .agents)
      let rows = OnboardingPresenter.agents(controller.snapshot).agents
      #expect(rows.first { $0.id == "opencode" }?.detail == "Restart opencode for this change to take effect.")
      #expect(rows.first?.detail != Words.claudeRestart)
      await controller.connectAgents(controller.agentsSwitchedOn)
      #expect(controller.snapshot.step == .done)
      #expect(commands.ran.count { $0.starts(with: ["agents", "install"]) } == 1)
    }
  }
}
