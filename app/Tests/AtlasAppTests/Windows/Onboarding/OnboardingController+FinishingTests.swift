import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import Testing

@testable import AtlasApp

@Suite("OnboardingController finishing")
struct OnboardingControllerFinishingTests {
  @Test("a check that ends once the window has closed changes nothing")
  func aLateCheckIsDropped() async throws {
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.checkingMac, "onboarding finish": #"{"ok": true}"#,
    ])
    let controller = OnboardingController.scripted(commands)
    commands.whileRunning { arguments in
      if arguments == Action.finishSetup.arguments { await controller.close() }
    }
    try await controller.whileOpen { running in
      try await settle("the check never ran") { commands.ran.contains(Action.finishSetup.arguments) }
      // run()'s own end is the sentinel: it ends once the check under way has done all it ever will.
      try await running.value("the check never ended")
      #expect(controller.snapshot.finish == .running, "the closed window took the check's outcome")
    }
  }

  @Test("the check runs once at a time: Try Again while it runs starts no second one")
  func theCheckRunsOnceAtATime() async throws {
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.checkingMac,
      "onboarding finish": #"{"ok": false, "error": "The helper did not answer"}"#,
    ])
    let controller = OnboardingController.scripted(commands)
    commands.whileRunning { arguments in
      if arguments == Action.finishSetup.arguments { controller.send(.retryFinish) }
    }
    try await controller.whileOpen { _ in
      try await settle("the check never ended") {
        controller.snapshot.finish == .failed("The helper did not answer")
      }
      #expect(commands.ran.filter { $0 == Action.finishSetup.arguments }.count == 1)
      await controller.close()
    }
  }

  @Test("leaving the check and coming back runs it again")
  func theCheckRunsAgainOnReturn() async throws {
    let shown = OnboardingReplies.checkingMac
      .replacingOccurrences(of: #""hidden": true"#, with: #""hidden": false"#)
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.checkingMac, "onboarding finish": #"{"ok": false, "error": "no answer"}"#,
    ])
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      try await settle("the check never failed") {
        controller.snapshot.finish != .idle && controller.snapshot.finish != .running
      }
      commands.answer("onboarding status", shown)
      controller.refresh()
      try await settle("the check was never left") { controller.snapshot.step == .switchToHelper }
      commands.answer("onboarding status", OnboardingReplies.checkingMac)
      controller.refresh()
      try await settle("the check never ran again") {
        commands.ran.filter { $0 == Action.finishSetup.arguments }.count == 2
      }
    }
  }

  @Test("Done sets Open at Login and opens Atlas, then the window closes")
  func doneClosesTheWindow() async {
    let commands = ScriptedCommands(["onboarding login": #"{"ok": true}"#, "menubar": #"{"ok": true}"#])
    let controller = OnboardingController.scripted(commands)
    controller.send(.setOpenAtLogin(false))
    await controller.complete()
    let done = commands.ran.filter { $0 != Action.onboardingStatus.arguments }
    #expect(done == [Action.setOpensAtLogin(false).arguments, Action.openMenuBar.arguments])
    #expect(controller.closeRequested)
  }

  @Test("Done that fails keeps the window open, saying why")
  func aFailedDoneKeepsTheWindow() async {
    let controller = OnboardingController.scripted(ScriptedCommands(["onboarding login": #"{"ok": true}"#]))
    await controller.complete()
    #expect(!controller.closeRequested)
    #expect(controller.snapshot.problem == "unknown command.")
  }
}
