import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import Testing

@testable import AtlasApp

@Suite("OnboardingController working line")
struct OnboardingControllerWorkingLineTests {
  /// A controller on the switch screen whose `atlas switch` takes `runs` on `clock`, answering `reply`.
  ///
  /// - Parameters:
  ///   - clock: The clock the command's time and the line's hold run on.
  ///   - runs: How long the command takes.
  ///   - reply: What it answers.
  /// - Returns: The controller.
  static func switching(_ clock: ManualClock, runs: Duration, reply: String) -> OnboardingController {
    let commands = ScriptedCommands(["onboarding status": OnboardingReplies.switchingMac, "switch": reply])
    commands.whileRunning { arguments in
      if arguments.first == "switch" { try? await clock.sleep(for: runs) }
    }
    return OnboardingController.scripted(commands, clock: clock)
  }

  @Test("a working line that has shown stays at least six tenths of a second, though its command ends sooner")
  func aShownLineStays() async throws {
    let clock = ManualClock()
    let controller = Self.switching(clock, runs: .milliseconds(400), reply: #"{"ok": true}"#)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      controller.send(.lockAndSwitch)
      try await clock.untilSleeping(for: .milliseconds(400), "the command never ran")
      clock.advance(by: .milliseconds(400))
      // Shown at 0.3 s, so it stays until 0.9 s: half a second more.
      try await clock.untilSleeping(for: .milliseconds(500), "the line was never held")
      #expect(controller.snapshot.working == Words.locking, "the line still shows")
      clock.advance(by: .milliseconds(500))
      try await settle("the line never went") { controller.snapshot.working == nil }
    }
  }

  @Test("a command that ends before its line shows leaves at once, and a failure shows at once")
  func aQuickCommandAndAFailureWaitForNothing() async throws {
    let clock = ManualClock()
    let quick = Self.switching(clock, runs: .milliseconds(200), reply: #"{"ok": true}"#)
    try await quick.whileOpen { _ in
      try await quick.untilLoaded()
      quick.send(.lockAndSwitch)
      try await clock.untilSleeping(for: .milliseconds(200), "the command never ran")
      clock.advance(by: .milliseconds(200))
      try await settle("the line was held though it never showed") { quick.snapshot.working == nil }
    }
    let failing = Self.switching(
      clock,
      runs: .milliseconds(400),
      reply:
        #"{"ok": false, "code": "not-ready", "error": "Atlas can't switch yet: macOS is still setting up an account."}"#
    )
    try await failing.whileOpen { _ in
      try await failing.untilLoaded()
      failing.send(.lockAndSwitch)
      try await clock.untilSleeping(for: .milliseconds(400), "the command never ran")
      clock.advance(by: .milliseconds(400))
      try await settle("the problem waited") { failing.snapshot.problem != nil }
      #expect(failing.snapshot.working == nil)
      #expect(
        failing.snapshot.problem == "Atlas can't switch yet: macOS is still setting up an account.",
        "a switch Atlas refused keeps its reason"
      )
    }
  }

  @Test("a screen that didn't lock says so plainly, with the keys that lock it")
  func aLockThatFailedSaysWhatToDo() async throws {
    let clock = ManualClock()
    let controller = Self.switching(
      clock,
      runs: .zero,
      reply:
        #"{"ok": false, "code": "switch", "error": "The screen could not be locked from here; lock it from the Apple menu"}"#
    )
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      await controller.lockAndSwitch()
      #expect(
        controller.snapshot.problem == "The screen didn’t lock. Press Control-Command-Q, then choose the atlas account."
      )
    }
  }
}
