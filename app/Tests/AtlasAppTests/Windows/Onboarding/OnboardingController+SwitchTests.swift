import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import Testing

@testable import AtlasApp

@Suite("OnboardingController switching")
struct OnboardingControllerSwitchTests {
  @Test(
    "switching back locks for the account that asked, or with none named when no one did",
    arguments: [("me", Action.switchTo("me")), (nil, .lockScreen)] as [(String?, Action)]
  )
  func switchingBack(asker: String?, action: Action) async throws {
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.helper(asker: asker, accessibility: true),
      "onboarding window": #"{"ok": true}"#, "switch": #"{"ok": true}"#,
    ])
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      await controller.switchBack()
      #expect(commands.ran.contains(action.arguments))
      #expect(controller.closeRequested)
    }
  }

  @Test("Back during Lock and Switch cancels the wait and goes back")
  func backDuringLockAndSwitch() async throws {
    let clock = ManualClock()
    let commands = ScriptedCommands(["onboarding status": OnboardingReplies.switchingMac, "switch": #"{"ok": true}"#])
    // The lock's wait: `atlas switch` waits up to 5 s for the screen to report locked.
    commands.whileRunning { arguments in
      if arguments.first == "switch" { try? await clock.sleep(for: .seconds(5)) }
    }
    let controller = OnboardingController.scripted(commands, clock: clock)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      #expect(controller.snapshot.step == .switchToHelper)
      controller.send(.lockAndSwitch)
      try await clock.untilSleeping(for: .seconds(5), "Lock and Switch never waited for the lock")
      #expect(controller.snapshot.working == Words.locking)
      let lock = try #require(controller.performing?.work, "Lock and Switch is under way")
      controller.send(.back)
      #expect(!clock.waiting.contains(.seconds(5)), "Back ended the lock's wait")
      try await lock.value("the stopped Lock and Switch never ended")
      #expect(controller.snapshot.working == nil)
      #expect(controller.snapshot.problem == nil, "a wait Back stopped is no failure")
      #expect(controller.snapshot.step == .switchToHelper)
      #expect(
        controller.state.footer.next?.isEnabled == true,
        "the switch is back as it was, Lock and Switch ready again"
      )
    }
  }
}
