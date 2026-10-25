import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("OnboardingController timing the helper's first login")
struct OnboardingControllerTimingTests {
  /// The markers a controller noted.
  @MainActor
  final class Marks {
    /// Each marker, in order.
    var all: [String] = []
  }

  /// A controller whose markers go to `marks`, over `commands` and `clock`.
  ///
  /// - Parameters:
  ///   - commands: The scripted command line.
  ///   - clock: The clock its waits and the timing run on.
  ///   - marks: Where its markers go.
  /// - Returns: The controller.
  static func timed(_ commands: ScriptedCommands, clock: ManualClock, marks: Marks) -> OnboardingController {
    OnboardingController.scripted(commands, clock: clock) { marks.all.append($0) }
  }

  @Test("after Lock and Switch, the helper answering and hiding are each noted once with the time since")
  func answeringAndHidingAreNoted() async throws {
    let clock = ManualClock()
    let marks = Marks()
    let commands = ScriptedCommands(["onboarding status": OnboardingReplies.switchingMac, "switch": #"{"ok": true}"#])
    let controller = Self.timed(commands, clock: clock, marks: marks)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      await controller.lockAndSwitch()
      clock.advance(by: .seconds(8.5))
      commands.answer("onboarding status", OnboardingReplies.checkingMac)
      controller.refresh()
      try await settle("the helper's answer was never read") { controller.snapshot.facts.helper.answering }
      #expect(marks.all == ["helper.answering after=8.5s", "helper.hidden after=8.5s"])
      controller.refresh()
      try await settle("the next reading never came") { commands.ran.count > 3 }
      #expect(marks.all.count == 2, "each is noted once")
    }
  }

  @Test("a first login silent for over three minutes says it is slow, and only on the switch screen")
  func aSilentFirstLoginSaysItIsSlow() async throws {
    let clock = ManualClock()
    let commands = ScriptedCommands(["onboarding status": OnboardingReplies.switchingMac, "switch": #"{"ok": true}"#])
    let controller = Self.timed(commands, clock: clock, marks: Marks())
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      await controller.lockAndSwitch()
      controller.timeFirstLogin()
      #expect(controller.state.note == .none, "not yet")
      clock.advance(by: .seconds(181))
      controller.timeFirstLogin()
      #expect(controller.state.note == .working(OnboardingWords.slowFirstLogin))
    }
  }
}
