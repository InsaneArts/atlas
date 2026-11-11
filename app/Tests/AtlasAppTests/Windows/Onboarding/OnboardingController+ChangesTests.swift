import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasOnboarding
import Testing

@testable import AtlasApp

@Suite("OnboardingController changes")
struct OnboardingControllerChangesTests {
  /// The readings of the facts `commands` ran.
  private static func readings(_ commands: ScriptedCommands) -> Int {
    commands.ran.count { $0 == Action.onboardingStatus.arguments }
  }

  @Test("a change macOS reports reads again, but never sooner than the gap after the last reading")
  func aChangeReadsAfterTheGap() async throws {
    let clock = ManualClock()
    let changes = ScriptedSetupChanges()
    let commands = ScriptedCommands(["onboarding status": OnboardingReplies.freshMac])
    let controller = OnboardingController.scripted(commands, clock: clock, changes: changes)
    try await controller.whileOpen { _ in
      try await clock.untilSleeping(for: ReadingPace.fallbackEvery, "the first reading never paused")
      clock.advance(by: .milliseconds(500))
      changes.change()
      try await clock.untilSleeping(for: .seconds(1), "the change did not wait for the rest of the gap")
      #expect(Self.readings(commands) == 1)
      clock.advance(by: .seconds(1))
      try await settle("the change never brought a reading") { Self.readings(commands) == 2 }
    }
  }

  @Test("a storm of changes reads once a gap, not once a change")
  func aStormReadsOnceAGap() async throws {
    let clock = ManualClock()
    let changes = ScriptedSetupChanges()
    let commands = ScriptedCommands(["onboarding status": OnboardingReplies.freshMac])
    let controller = OnboardingController.scripted(commands, clock: clock, changes: changes)
    try await controller.whileOpen { _ in
      try await clock.untilSleeping(for: ReadingPace.fallbackEvery, "the first reading never paused")
      for _ in 0..<50 {
        changes.change()
        await afterQueuedWork()
      }
      try await clock.untilSleeping(for: ReadingPace.minimumGap, "the storm did not wait for the gap")
      #expect(Self.readings(commands) == 1)
      clock.advance(by: ReadingPace.minimumGap)
      try await settle("the storm never brought a reading") { Self.readings(commands) == 2 }
      try await clock.untilSleeping(for: ReadingPace.minimumGap, "the next reading never paused")
      #expect(Self.readings(commands) == 2, "fifty changes, one reading")
    }
  }

  @Test("a change ends the wait while the person is away, and reads nothing until they are back")
  func aChangeEndsTheAwayWait() async throws {
    let clock = ManualClock()
    let changes = ScriptedSetupChanges()
    var onScreen = true
    let commands = ScriptedCommands(["onboarding status": OnboardingReplies.checkingMac])
    let controller = OnboardingController.scripted(
      commands,
      clock: clock,
      console: SessionConsole(read: { onScreen }),
      consoleUser: { onScreen ? "me" : "loginwindow" },
      changes: changes
    )
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      onScreen = false
      changes.change()
      try await clock.untilSleeping(for: ReadingPace.minimumGap, "the away wait never began")
      changes.change()
      try await clock.untilSleeping(for: ReadingPace.minimumGap, "a change while away read something")
      #expect(Self.readings(commands) == 1)
      onScreen = true
      changes.change()
      try await clock.untilSleeping(for: OnboardingController.quietAfterSwitch, "the return never settled")
      clock.advance(by: OnboardingController.quietAfterSwitch)
      try await settle("the first reading back never came") { Self.readings(commands) == 2 }
    }
  }

  @Test("the check does not start when this login left the screen between its readings")
  func leavingTheScreenRestartsTheSettling() async throws {
    let clock = ManualClock()
    let changes = ScriptedSetupChanges()
    var onScreen = true
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.checkingMac, "onboarding finish": #"{"ok": true}"#,
    ])
    let controller = OnboardingController.scripted(
      commands,
      clock: clock,
      console: SessionConsole(read: { onScreen }),
      consoleUser: { onScreen ? "me" : "atlas" },
      changes: changes
    )
    try await controller.whileOpen { _ in
      try await clock.untilSleeping(for: OnboardingController.settledFor, "the first reading never paused")
      // This login leaves the screen and comes back before the reading that would start the check: macOS says so as it
      // happens, and the change it reports comes after.
      onScreen = false
      controller.dependencies.console.refresh()
      onScreen = true
      changes.change()
      try await clock.untilSleeping(1, "the loop never paused again")
      clock.advance(by: OnboardingController.settledFor)
      try await settle("the next reading never came") { Self.readings(commands) == 2 }
      try await clock.untilSleeping(1, "the loop never paused after it")
      #expect(!commands.ran.contains(Action.finishSetup.arguments), "the check started on a broken stay")
      clock.advance(by: OnboardingController.settledFor)
      try await settle("the check never ran once settled") { commands.ran.contains(Action.finishSetup.arguments) }
    }
  }

  @Test("the window watches the helper's acceptance folder once a reading names the helper's account")
  func theWindowWatchesTheAcceptanceFolder() async throws {
    let changes = ScriptedSetupChanges()
    let named = OnboardingReplies.switchingMac
      .replacingOccurrences(of: #""exists": true"#, with: #""exists": true, "uid": 502"#)
    let controller = OnboardingController.scripted(
      ScriptedCommands(["onboarding status": named]),
      changes: changes
    )
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      #expect(changes.watched == [["/Users/Shared/Atlas-accept-502"]])
    }
  }

  @Test("in the helper the window watches its job's folder")
  func theHelperWatchesItsJobFolder() async throws {
    let changes = ScriptedSetupChanges()
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.helper(accessibility: false), "onboarding window": #"{"ok": true}"#,
    ])
    let controller = OnboardingController.scripted(commands, changes: changes)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      #expect(changes.watched == [["/Users/me/.atlas/helper"]])
    }
  }

  @Test("a window whose run is cancelled before it closes still stops the watcher")
  func cancellingStopsTheWatcher() async throws {
    let changes = ScriptedSetupChanges()
    let controller = OnboardingController.scripted(
      ScriptedCommands(["onboarding status": OnboardingReplies.freshMac]),
      changes: changes
    )
    let running = Task { await controller.run() }
    try await controller.untilLoaded()
    running.cancel()
    await running.value
    #expect(changes.stopped)
  }

  @Test("closing the window stops the watcher")
  func closingStopsTheWatcher() async throws {
    let changes = ScriptedSetupChanges()
    let controller = OnboardingController.scripted(
      ScriptedCommands(["onboarding status": OnboardingReplies.freshMac]),
      changes: changes
    )
    try await controller.whileOpen { running in
      try await controller.untilLoaded()
      await controller.close()
      try await running.value("run() never ended once the window closed")
    }
    #expect(changes.stopped)
  }
}
