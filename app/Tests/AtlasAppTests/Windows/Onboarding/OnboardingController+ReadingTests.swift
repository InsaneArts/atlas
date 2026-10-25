import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("OnboardingController reading")
struct OnboardingControllerReadingTests {
  @Test("a closed window starts no reading")
  func aClosedWindowStartsNothing() async throws {
    let commands = ScriptedCommands(["onboarding status": OnboardingReplies.freshMac])
    let controller = OnboardingController.scripted(commands)
    await controller.close()
    controller.refresh()
    controller.send(.checkAgain)
    controller.send(.continueFinishing)
    // Unstructured, so the ceiling bounds it: a closed window's run() returns at once, having started nothing.
    try await Task { await controller.run() }
      .value("a closed window's run() never returned")
    // The sentinel goes the same way: an open window over the same command line reads once, after any reading the
    // closed one started has set off for the command line.
    let open = OnboardingController.scripted(commands)
    try await open.whileOpen { _ in try await open.untilLoaded() }
    #expect(commands.ran == [Action.onboardingStatus.arguments], "only the open window read")
    #expect(!controller.snapshot.loaded)
  }

  @Test("closing the helper's window clears its mark only once a mark under way has settled")
  func closingClearsTheMarkLast() async throws {
    let open = Action.markWindow(open: true).arguments
    let closed = Action.markWindow(open: false).arguments
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.helper(accessibility: false), "onboarding window": #"{"ok": true}"#,
    ])
    let controller = OnboardingController.scripted(commands)
    let overlap = Overlap()
    // The person closes the window while the open mark runs; a close that did not wait for the mark would have
    // cleared it before the sentinel queued behind the close.
    commands.whileRunning { arguments in
      guard arguments == open else { return }
      Task { await controller.close() }
      // Once the close has run to its wait, a command sent the same way: a clearing sent meanwhile lands first.
      await afterQueuedWork()
      let probe: any CommandRunning = commands
      _ = await probe.run(["probe"], input: nil)
      overlap.seen = commands.ran.contains(closed)
    }
    try await controller.whileOpen { _ in
      try await settle("the mark was never cleared") { overlap.seen != nil && commands.ran.contains(closed) }
    }
    #expect(overlap.seen == false, "the window was marked closed while the open mark still ran")
    #expect(commands.ran.last == closed)
  }

  @Test("a blocked window marks itself open in the helper, where the opener waits for it, and not in your account")
  func aBlockedWindowMarksItselfOnlyInTheHelper() async throws {
    let open = Action.markWindow(open: true).arguments
    let unsigned = OnboardingReplies.helper(accessibility: false)
      .replacingOccurrences(of: #""signed": true"#, with: #""signed": false"#)
    let helper = ScriptedCommands(["onboarding status": unsigned, "onboarding window": #"{"ok": true}"#])
    let blocked = OnboardingController.scripted(helper)
    try await blocked.whileOpen { _ in
      try await blocked.untilLoaded()
      guard case .blocked = blocked.snapshot.step else {
        Issue.record("an unsigned Atlas in the helper is blocked")
        return
      }
      try await settle("the blocked window was never marked") { helper.ran.contains(open) }
    }

    let mine = OnboardingReplies.freshMac
      .replacingOccurrences(of: #""signed": true"#, with: #""signed": false"#)
    let person = ScriptedCommands(["onboarding status": mine, "onboarding window": #"{"ok": true}"#])
    let yours = OnboardingController.scripted(person)
    try await yours.whileOpen { _ in
      try await yours.untilLoaded()
      guard case .blocked = yours.snapshot.step else {
        Issue.record("an unsigned Atlas in your account is blocked")
        return
      }
      // A second reading: a mark the first one started would have reached the command line before it.
      yours.refresh()
      try await settle("the facts were never read again") {
        person.ran.filter { $0 == Action.onboardingStatus.arguments }.count == 2
      }
      #expect(!person.ran.contains(open))
    }
  }

  @Test("a failed first reading's message goes once a reading succeeds")
  func aFailedFirstReadingIsForgotten() async throws {
    let commands = ScriptedCommands([:])
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await settle("the failed reading was never shown") { controller.snapshot.problem != nil }
      commands.answer("onboarding status", OnboardingReplies.freshMac)
      controller.refresh()
      try await controller.untilLoaded()
      #expect(controller.snapshot.problem == nil)
    }
  }

  @Test("Open at Login starts from what the login item says once the person has chosen")
  func openAtLoginStartsFromTheEarlierChoice() async throws {
    let decided = OnboardingReplies.freshMac
      .replacingOccurrences(
        of: #""configured": false"#,
        with: #""menubarLogin": true, "configured": false"#
      )
    let controller = OnboardingController.scripted(ScriptedCommands(["onboarding status": decided]))
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      #expect(!controller.snapshot.opensAtLogin, "the login item is off, as the person left it")
    }
  }

  @Test("the setup window reads again after its interval, and not before")
  func theWindowReadsAgainAfterItsInterval() async throws {
    let clock = ManualClock()
    let commands = ScriptedCommands(["onboarding status": OnboardingReplies.freshMac])
    let controller = OnboardingController.scripted(commands, clock: clock)
    try await controller.whileOpen { _ in
      try await clock.untilSleeping(for: OnboardingController.readEvery, "the first reading never paused")
      #expect(commands.ran == [Action.onboardingStatus.arguments])
      #expect(clock.waiting == [OnboardingController.readEvery])
      clock.advance(by: OnboardingController.readEvery - .milliseconds(1))
      #expect(clock.waiting == [.milliseconds(1)], "the pause has not ended a moment before its interval")
      clock.advance(by: .milliseconds(1))
      try await settle("the facts were never read again") { commands.ran.count == 2 }
    }
  }

  @Test("a job restarting to pick up Screen Recording keeps its permissions and the permissions screen")
  func aRestartingJobKeepsItsPermissions() async throws {
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.helper(accessibility: true), "onboarding window": #"{"ok": true}"#,
    ])
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      commands.answer("onboarding status", OnboardingReplies.restarting)
      controller.refresh()
      try await settle("the facts were never read again") {
        commands.ran.filter { $0 == Action.onboardingStatus.arguments }.count == 2
      }
      // The reading answers a turn after it is asked, on the main actor: behind it, its facts have landed.
      await afterQueuedWork()
      #expect(controller.snapshot.step == .permissions, "not Allow again")
      #expect(controller.snapshot.facts.asker.accessibility, "not off")
    }
  }

  @Test("a job that answers nothing for too long counts as stopped")
  func aStoppedJobAsksForAllowAgain() async throws {
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.helper(accessibility: true), "onboarding window": #"{"ok": true}"#,
    ])
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      commands.answer("onboarding status", OnboardingReplies.restarting)
      for reading in 2...(OnboardingController.restartReadings + 2) {
        controller.refresh()
        try await settle("the facts were never read again") {
          commands.ran.filter { $0 == Action.onboardingStatus.arguments }.count == reading
        }
      }
      try await settle("the job never counted as stopped") { controller.snapshot.step == .allow }
    }
  }

  @Test(
    "polling, Check Again and Continue Finishing update the screen while a queued action is still running",
    arguments: [nil, .checkAgain, .continueFinishing] as [OnboardingEvent?]
  )
  func aReadingLandsDuringAnAction(refresh: OnboardingEvent?) async throws {
    let clock = ManualClock()
    let gate = ReleaseSignal()
    let commands = ScriptedCommands(["onboarding status": OnboardingReplies.switchingMac, "switch": #"{"ok": true}"#])
    commands.whileRunning { arguments in
      if arguments.first == "switch" { await gate.wait() }
    }
    let controller = OnboardingController.scripted(commands, clock: clock)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      try await clock.untilSleeping(for: OnboardingController.readEvery, "the first reading never paused")
      controller.send(.lockAndSwitch)
      try await settle("Lock and Switch never ran") { controller.snapshot.working == Words.locking }
      commands.answer(
        "onboarding status",
        OnboardingReplies.switchingMac
          .replacingOccurrences(of: #""signedIn": false"#, with: #""signedIn": true"#)
      )
      // Refresh events take the reading lane while Lock and Switch still holds the action queue.
      if let refresh {
        controller.send(refresh)
      } else {
        clock.advance(by: OnboardingController.readEvery)
      }
      try await settle("the reading never landed while Lock and Switch ran") {
        controller.snapshot.facts.helper.signedIn
      }
      #expect(controller.snapshot.working == Words.locking, "Lock and Switch still runs")
      gate.open()
      try await settle("Lock and Switch never ended") { controller.snapshot.working == nil }
    }
  }

  @Test("closing the setup window cancels the queue and the reading loop, and runs nothing queued")
  func closingCancelsTheQueue() async throws {
    let clock = ManualClock()
    let gate = ReleaseSignal()
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.freshMac, "onboarding forget": #"{"ok": true}"#,
      "onboarding login": #"{"ok": true}"#, "menubar": #"{"ok": true}"#,
    ])
    commands.whileRunning { arguments in
      if arguments == Action.forgetHelper.arguments { await gate.wait() }
    }
    let controller = OnboardingController.scripted(commands, clock: clock)
    try await controller.whileOpen { running in
      try await controller.untilLoaded()
      try await clock.untilSleeping(for: OnboardingController.readEvery, "the first reading never paused")
      controller.send(.chooseAnother)
      try await settle("Choose Another never started") { commands.ran.contains(Action.forgetHelper.arguments) }
      controller.send(.complete)
      // The check runs its command without the words an action shows: only the queue keeps it back.
      controller.send(.retryFinish)
      await controller.close()
      #expect(clock.waiting.isEmpty, "the reading loop's pause ended with the window")
      gate.open()
      // run()'s own end is the sentinel: it ends once the command under way has settled and the queue has stopped.
      try await running.value("run() never ended once the window closed")
    }
    #expect(!commands.ran.contains { $0.starts(with: ["onboarding", "login"]) }, "Done, queued behind it, never ran")
    #expect(!commands.ran.contains(Action.openMenuBar.arguments))
    #expect(!commands.ran.contains(Action.finishSetup.arguments), "nor did the check queued after it")
    #expect(commands.ran.filter { $0 == Action.onboardingStatus.arguments }.count == 1, "no reading after the close")
  }
}
