import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import Testing

@testable import AtlasApp

@Suite("Setup Claude connections")
struct OnboardingClaudeTests {
  @Test("setup connects both installed Claude parts from its one row and shows a part failure there")
  func connectsBothParts() async throws {
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.finishedMac,
      "agents status":
        #"{"ok":true,"agents":[{"agent":"claude","installed":true,"connected":false},{"agent":"claude-app","installed":true,"connected":false}]}"#,
      "agents install":
        #"{"ok":false,"agents":[{"agent":"claude","ok":true},{"agent":"claude-app","ok":false,"error":"could not write settings"}]}"#,
    ])
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      try await settle("the agents were never read") { controller.snapshot.agentsLoaded }
      let screen = OnboardingPresenter.agents(controller.snapshot)
      #expect(screen.agents.map(\.title) == ["Claude", "Codex"])
      #expect(screen.agents.first?.detail.contains("Claude Code and the Claude app") == true)
      await controller.connectAgents(controller.agentsSwitchedOn)
      #expect(commands.ran.contains(["agents", "install", "--claude", "--app"]))
      #expect(controller.snapshot.agents.first?.connected == false)
      #expect(OnboardingPresenter.agents(controller.snapshot).agents.first?.detail.contains("The Claude app") == true)
      #expect(!controller.snapshot.choices.agentsDone)
    }
  }
}
