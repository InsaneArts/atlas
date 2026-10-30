import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import Testing

@testable import AtlasApp

@Suite("OnboardingController switching")
struct OnboardingControllerSwitchTests {
  @Test("switch-back and lock-screen requests refuse a session that went off-console after the button was drawn")
  func switchingBackRechecksTheConsole() async throws {
    for asker in ["me", nil] as [String?] {
      var onConsole = true
      let console = SessionConsole(read: { onConsole })
      let commands = ScriptedCommands([
        "onboarding status": OnboardingReplies.helper(asker: asker, accessibility: true, screenCapture: true),
        "onboarding window": #"{"ok": true}"#,
      ])
      let controller = OnboardingController.scripted(commands, console: console)
      try await controller.whileOpen { _ in
        try await controller.untilLoaded()
        #expect(controller.state.footer.next?.isEnabled == true)
        onConsole = false
        await controller.switchBack()
        #expect(!commands.ran.contains { $0.first == "switch" })
        #expect(controller.snapshot.problem == SessionConsole.refusal)
        #expect(!controller.closeRequested)
        #expect(controller.state.footer.next?.isEnabled == false)
        await controller.lockAndSwitch()
        #expect(!commands.ran.contains { $0.first == "switch" })
      }
    }
  }

  @Test("opening setup, choosing Not Now in the helper, returning and reopening keeps the manual opening instructions")
  func returningAfterNotNowKeepsTheOpeningInstructions() async throws {
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.freshMac,
      "onboarding create": #"{"ok": true, "readied": true}"#,
    ])
    commands.whileRunning { arguments in
      if arguments.starts(with: ["onboarding", "create"]) {
        commands.answer("onboarding status", OnboardingReplies.switchingMac)
      }
    }
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      controller.send(.begin)
      controller.send(.pick(.newAccount))
      await controller.useHelper()
      var form = controller.snapshot.form
      form.password = "hunter2"
      form.verify = "hunter2"
      controller.update(form)
      await controller.createAccount()
      try await settle("the helper was never read") { controller.snapshot.step == .switchToHelper }
      #expect(controller.snapshot.helperReadied)
      #expect(OnboardingPresenter.switchToHelper(controller.snapshot).opensByItself)
      #expect(OnboardingPresenter.switchToHelper(controller.snapshot).showsManualOpening)

      let helperCommands = ScriptedCommands([
        "onboarding status": OnboardingReplies.restarting, "onboarding window": #"{"ok": true}"#,
      ])
      let helper = OnboardingController.scripted(helperCommands)
      try await helper.whileOpen { _ in
        try await helper.untilLoaded()
        try await settle("the helper's window never opened") {
          helperCommands.ran.contains(Action.markWindow(open: true).arguments)
        }
        helper.send(.notNow)
        #expect(helper.closeRequested)
        await helper.close()
      }

      let returned = OnboardingReplies.switchingMac
        .replacingOccurrences(of: #""signedIn": false"#, with: #""signedIn": true, "hidden": true"#)
      commands.answer("onboarding status", returned)
      controller.refresh()
      try await settle("the return was never read") { controller.snapshot.facts.helper.signedIn }
      #expect(controller.snapshot.helperReadied)
      #expect(OnboardingPresenter.switchToHelper(controller.snapshot).opensByItself)
      #expect(OnboardingPresenter.switchToHelper(controller.snapshot).showsManualOpening)
      await controller.close()

      let reopened = OnboardingController.scripted(commands)
      try await reopened.whileOpen { _ in
        try await reopened.untilLoaded()
        #expect(reopened.snapshot.step == .switchToHelper)
        #expect(reopened.snapshot.helperReadied)
        #expect(OnboardingPresenter.switchToHelper(reopened.snapshot).opensByItself)
        #expect(OnboardingPresenter.switchToHelper(reopened.snapshot).showsManualOpening)
      }
    }
  }

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
