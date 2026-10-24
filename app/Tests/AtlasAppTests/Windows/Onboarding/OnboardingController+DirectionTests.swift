import AtlasCommandsTesting
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("OnboardingController direction")
struct OnboardingControllerDirectionTests {
  /// The setup window's sample facts and snapshots, shared by the onboarding suites.
  typealias Samples = OnboardingSamples

  @Test("Back slides the window's screens back, and moving on slides them forward again")
  func backSlidesBack() throws {
    let controller = OnboardingController.scripted(ScriptedCommands([:]))
    controller.snapshot = Samples.snapshot(try Samples.person(nil))
    controller.send(.begin)
    #expect(controller.snapshot.step == .chooseHelper)
    #expect(controller.state.direction == .forward)
    controller.send(.back)
    #expect(controller.snapshot.step == .welcome)
    #expect(controller.state.direction == .back)
    controller.send(.pick(.newAccount))
    #expect(controller.state.direction == .back, "a change on the same screen keeps the way it came")
    controller.send(.begin)
    #expect(controller.state.direction == .forward)
  }
}
