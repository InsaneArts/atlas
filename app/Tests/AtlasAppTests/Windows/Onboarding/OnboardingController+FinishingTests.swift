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
      try await controller.untilSettled(commands)
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
      try await controller.untilSettled(commands)
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
      try await controller.untilSettled(commands)
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

  @Test("the check waits until this login has been read on screen twice in a row")
  func theCheckWaitsForTwoReadingsOnScreen() async throws {
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.checkingMac, "onboarding finish": #"{"ok": true}"#,
    ])
    let clock = ManualClock()
    let controller = OnboardingController.scripted(commands, clock: clock)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      // The pause follows the reading's whole outcome, a check it started included.
      try await clock.untilSleeping(for: OnboardingController.readEvery, "the first reading never paused")
      #expect(controller.snapshot.step == .finishing)
      #expect(controller.snapshot.finish == .idle, "the check started on the first reading")
      try await controller.untilSettled(commands)
      try await settle("the check never ran") { commands.ran.contains(Action.finishSetup.arguments) }
    }
  }

  @Test("while the person is away nothing is read, even on a refresh; back, the check waits two readings")
  func nothingIsReadWhileAway() async throws {
    let clock = ManualClock()
    var onScreen = true
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.checkingMac, "onboarding finish": #"{"ok": true}"#,
    ])
    let controller = OnboardingController.scripted(
      commands,
      clock: clock,
      console: SessionConsole(read: { onScreen }),
      consoleUser: { onScreen ? "me" : "loginwindow" }
    )
    let readings = { commands.ran.filter { $0 == Action.onboardingStatus.arguments }.count }
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      onScreen = false
      for _ in 0..<3 {
        try await clock.untilSleeping(for: OnboardingController.readEvery, "the readings never paused")
        clock.advance(by: OnboardingController.readEvery)
      }
      try await clock.untilSleeping(for: OnboardingController.readEvery, "the away wait never paused")
      controller.refresh()
      try await clock.untilSleeping(for: OnboardingController.readEvery, "a refresh ended the away wait")
      clock.advance(by: OnboardingController.readEvery)
      try await clock.untilSleeping(for: OnboardingController.readEvery, "the away wait never paused again")
      #expect(readings() == 1, "a reading ran while the person was away")
      onScreen = true
      clock.advance(by: OnboardingController.readEvery)
      try await clock.untilSleeping(for: OnboardingController.quietAfterSwitch, "the return never settled")
      controller.refresh()
      clock.advance(by: .seconds(1))
      try await clock.untilSleeping(for: .seconds(2), "a refresh cut the return's quiet pause short")
      #expect(readings() == 1, "a reading ran before the return settled")
      clock.advance(by: .seconds(2))
      try await settle("the first reading back never came") { readings() == 2 }
      try await clock.untilSleeping(for: OnboardingController.readEvery, "the readings never paused")
      #expect(controller.snapshot.finish == .idle, "the check started on the first reading back")
      clock.advance(by: OnboardingController.readEvery)
      try await settle("the check never ran once back") { commands.ran.contains(Action.finishSetup.arguments) }
    }
  }

  @Test("the readings carry on while the helper has the screen, as they drive its setup there")
  func readingsCarryOnInTheHelper() async throws {
    let clock = ManualClock()
    let commands = ScriptedCommands(["onboarding status": OnboardingReplies.checkingMac])
    let controller = OnboardingController.scripted(
      commands,
      clock: clock,
      console: SessionConsole(read: { false }),
      consoleUser: { "atlas" }
    )
    let readings = { commands.ran.filter { $0 == Action.onboardingStatus.arguments }.count }
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      for count in 2...3 {
        try await clock.untilSleeping(for: OnboardingController.readEvery, "the readings never paused")
        clock.advance(by: OnboardingController.readEvery)
        try await settle("reading \(count) never came") { readings() == count }
      }
      #expect(controller.snapshot.finish == .idle, "the check started while the helper had the screen")
    }
  }

  @Test("the passed check's Continue lights up and takes presses only once the pass has shown a moment")
  func continueFollowsThePass() async throws {
    let clock = ManualClock()
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.checkingMac, "onboarding finish": #"{"ok": true}"#,
    ])
    let controller = OnboardingController.scripted(commands, clock: clock)
    try await controller.whileOpen { _ in
      try await controller.untilSettled(commands)
      try await settle("the check never passed") { controller.snapshot.finish == .passed }
      #expect(controller.state.footer.next?.isEnabled == false, "Continue waits while the pass shows")
      try await clock.untilSleeping(for: OnboardingController.continueFollows, "Continue never waited")
      clock.advance(by: OnboardingController.continueFollows)
      try await settle("Continue never lit up") { controller.state.footer.next?.isEnabled == true }
      #expect(controller.state.footer.next?.event == .continueFinishing)
    }
  }

  @Test("Done sets Open at Login, keeps the home folder private and opens Atlas, then the window closes")
  func doneClosesTheWindow() async {
    let commands = ScriptedCommands([
      "onboarding login": #"{"ok": true}"#,
      "onboarding files": Self.privateHome,
      "menubar": #"{"ok": true}"#,
    ])
    let controller = OnboardingController.scripted(commands)
    controller.send(.setOpenAtLogin(false))
    await controller.complete()
    let done = commands.ran.filter { $0 != Action.onboardingStatus.arguments }
    #expect(
      done == [Action.setOpensAtLogin(false).arguments, Action.makeHomePrivate.arguments, Action.openMenuBar.arguments]
    )
    #expect(controller.closeRequested)
  }

  @Test("with the box unchecked Done only remembers the offer, and leaves the home folder as it is")
  func anUncheckedBoxOnlyRemembersTheOffer() async {
    let commands = ScriptedCommands([
      "onboarding login": #"{"ok": true}"#,
      "onboarding files": Self.privateHome,
      "menubar": #"{"ok": true}"#,
    ])
    let controller = OnboardingController.scripted(commands)
    controller.send(.setKeepHomePrivate(false))
    await controller.complete()
    #expect(commands.ran.contains(Action.homeOffered.arguments))
    #expect(!commands.ran.contains(Action.makeHomePrivate.arguments))
    #expect(controller.closeRequested)
  }

  @Test("while Done remembers the offer, the switches wait: a box ticked meanwhile isn't taken")
  func theSwitchesWaitWhileTheOfferIsRemembered() async throws {
    let gate = ReleaseSignal()
    let commands = ScriptedCommands([
      "onboarding login": #"{"ok": true}"#,
      "onboarding files": Self.privateHome,
      "menubar": #"{"ok": true}"#,
    ])
    commands.whileRunning { arguments in
      if arguments == Action.homeOffered.arguments { await gate.wait() }
    }
    let controller = OnboardingController.scripted(commands)
    controller.send(.setKeepHomePrivate(false))
    let completing = Task { await controller.complete() }
    try await settle("the offer was never remembered") { commands.ran.contains(Action.homeOffered.arguments) }
    #expect(!OnboardingPresenter.done(controller.snapshot).isEnabled)
    controller.send(.setKeepHomePrivate(true))
    #expect(!controller.snapshot.keepsHomePrivate)
    gate.open()
    await completing.value
    #expect(!controller.snapshot.keepsHomePrivate)
    #expect(!commands.ran.contains(Action.makeHomePrivate.arguments))
    #expect(controller.closeRequested)
  }

  @Test("a home folder Atlas couldn't make private stops Done, saying why; the next Done finishes, still unoffered")
  func aFailedPrivacyStopsDoneOnce() async {
    let commands = ScriptedCommands([
      "onboarding login": #"{"ok": true}"#,
      "onboarding files":
        #"{"ok": false, "code": "io", "error": "Atlas couldn't make your home folder private. In Terminal, run: chmod +a# 0 \"user:atlas deny list,search\" ~"}"#,
      "menubar": #"{"ok": true}"#,
    ])
    let controller = OnboardingController.scripted(commands)
    await controller.complete()
    #expect(!controller.closeRequested)
    #expect(!controller.snapshot.keepsHomePrivate)
    #expect(controller.snapshot.problem?.hasSuffix("You can make it private later in Settings › Privacy.") == true)
    #expect(OnboardingPresenter.done(controller.snapshot).keepsHomePrivate == false)
    await controller.complete()
    #expect(controller.closeRequested)
    #expect(!commands.ran.contains(Action.homeOffered.arguments), "a failed attempt stays unoffered")
  }

  @Test("Done that fails keeps the window open, saying why")
  func aFailedDoneKeepsTheWindow() async {
    let controller = OnboardingController.scripted(
      ScriptedCommands(["onboarding login": #"{"ok": true}"#, "onboarding files": Self.privateHome])
    )
    await controller.complete()
    #expect(!controller.closeRequested)
    #expect(controller.snapshot.problem == "unknown command.")
  }

  /// The reply of a home folder made private.
  static let privateHome =
    #"{"ok": true, "private": true, "readable": 0, "examples": [], "offered": true, "signInCopies": false}"#

  @Test("Open at Login keeps its saved choice while Done waits for Atlas to open", arguments: [true, false])
  func theLoginChoiceWaitsForDone(started: Bool) async throws {
    let gate = ReleaseSignal()
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.finishedMac,
      "onboarding login": #"{"ok": true}"#,
      "onboarding files": Self.privateHome,
      "menubar": started ? #"{"ok": true}"# : #"{"ok": false, "error": "Atlas could not open"}"#,
    ])
    commands.whileRunning { arguments in
      if arguments == Action.openMenuBar.arguments { await gate.wait() }
    }
    let controller = OnboardingController.scripted(commands)
    controller.snapshot.choices.agentsDone = true
    try await controller.whileOpen { _ in
      defer { gate.open() }
      try await controller.untilLoaded()
      controller.send(.setOpenAtLogin(false))
      #expect(OnboardingPresenter.done(controller.snapshot).isEnabled)
      controller.send(.complete)
      try await settle("Atlas never started opening") { commands.ran.contains(Action.openMenuBar.arguments) }
      #expect(commands.ran.contains(Action.setOpensAtLogin(false).arguments))
      #expect(controller.snapshot.working == OnboardingWords.opening)
      #expect(!OnboardingPresenter.done(controller.snapshot).isEnabled)
      controller.send(.setOpenAtLogin(true))
      #expect(!controller.snapshot.opensAtLogin)
      let completing = try #require(controller.performing?.work)
      gate.open()
      try await completing.value("Done never ended")
      #expect(!controller.snapshot.opensAtLogin)
      #expect(controller.closeRequested == started)
      if !started {
        #expect(OnboardingPresenter.done(controller.snapshot).isEnabled)
        controller.send(.setOpenAtLogin(true))
        #expect(controller.snapshot.opensAtLogin)
      }
    }
  }
}
