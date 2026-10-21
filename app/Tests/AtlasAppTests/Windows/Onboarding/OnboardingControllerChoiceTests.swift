import AtlasKit
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

/// The helper `atlas` asked and never answered, so it can be given up.
private let choiceAsked = """
  {"ok": true, "role": "controller", "me": {"name": "me"}, "app": {"installed": true, "signed": true},
   "configured": true, "candidates": [],
   "helper": {"exists": true, "name": "atlas", "administrator": false, "paired": true, "forgettable": true,
              "signedIn": false, "answering": false}}
  """

/// The helper given up: no longer asked, and listed among the accounts to choose again.
private let choiceForgotten = """
  {"ok": true, "role": "controller", "me": {"name": "me"}, "app": {"installed": true, "signed": true},
   "configured": true, "candidates": [{"name": "atlas"}, {"name": "qa"}],
   "helper": {"exists": true, "name": "atlas", "administrator": false, "paired": false, "forgettable": false,
              "signedIn": false, "answering": false}}
  """

@MainActor
@Suite("OnboardingController choosing again")
struct OnboardingControllerChoiceTests {
  @Test("choosing another account after creating one goes back to the list, not the account form")
  func chooseAnotherAfterCreating() async throws {
    let commands = ScriptedCommands(["onboarding status": freshMac, "onboarding forget": #"{"ok": true}"#])
    let controller = OnboardingControllerTests.controller(commands)
    controller.refresh()
    try await settle("the facts were never read") { controller.snapshot.loaded }
    controller.send(.begin)
    await controller.useHelper()
    #expect(controller.snapshot.step == .createAccount)
    commands.answer("onboarding status", choiceAsked)
    controller.refresh()
    try await settle("the switch step never showed") { controller.snapshot.step == .switchToHelper }
    #expect(controller.state.footer.back?.title == .chooseAnother)
    commands.answer("onboarding status", choiceForgotten)
    controller.send(.chooseAnother)
    try await settle("the helper was never given up") {
      commands.ran.contains(["onboarding", "forget"]) && controller.snapshot.step != .switchToHelper
    }
    #expect(controller.snapshot.step == .chooseHelper, "landed on \(controller.snapshot.step)")
  }

  @Test("skipping agents that could not be checked leaves no note about them on Done")
  func skippedAgentsLeavesNoProblem() async throws {
    let commands = ScriptedCommands(["onboarding status": finishedMac])
    let controller = OnboardingControllerTests.controller(commands)
    controller.refresh()
    try await settle("the agents check never failed") { controller.snapshot.agentsFailed }
    controller.send(.connectAgents)
    try await settle("Done never showed") { controller.snapshot.step == .done }
    #expect(controller.snapshot.problem == nil, "Done shows: \(controller.snapshot.problem ?? "")")
  }
}
