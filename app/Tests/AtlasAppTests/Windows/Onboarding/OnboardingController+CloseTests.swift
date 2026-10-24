import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("OnboardingController closing a deleted account's login")
struct OnboardingControllerCloseTests {
  /// A finished Mac where a deleted account's login is still open.
  static let leftoverMac = OnboardingReplies.finishedMac
    .replacingOccurrences(
      of: #""candidates": []"#,
      with: #""candidates": [], "leftoverLogins": [{"uid": 502, "kind": "orphan"}]"#
    )

  @Test(
    "fresh setup with a leftover login asks for authorization once for either a new or an existing helper",
    arguments: [true, false]
  )
  func freshSetupApprovesOnce(create: Bool) async throws {
    let fresh = OnboardingReplies.freshMac
      .replacingOccurrences(
        of: #""candidates": []"#,
        with: #""candidates": [{"name": "qa"}], "leftoverLogins": [{"uid": 502, "kind": "orphan"}]"#
      )
    let commands = ScriptedCommands([
      "onboarding status": fresh,
      "onboarding create": #"{"ok": true, "readied": true}"#,
      "onboarding choose": #"{"ok": true, "readied": true}"#,
    ])
    // Each privileged command stands in for its authorization call; neither polling nor choosing New asks.
    var authorizations = 0
    commands.whileRunning { arguments in
      if arguments.starts(with: ["onboarding", "create"])
        || arguments.starts(with: ["onboarding", "choose"])
        || arguments.starts(with: ["onboarding", "close"])
      {
        authorizations += 1
        commands.answer("onboarding status", OnboardingReplies.switchingMac)
      }
    }
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      #expect(controller.snapshot.step == .welcome)
      controller.send(.begin)
      #expect(controller.snapshot.step == .chooseHelper)
      await controller.closeLeftovers()
      #expect(authorizations == 0, "a stale standalone cleanup request cannot add an approval to setup")
      controller.send(.pick(create ? .newAccount : .existing("qa")))
      await controller.useHelper()
      if create {
        #expect(controller.snapshot.step == .createAccount)
        #expect(authorizations == 0)
        var form = controller.snapshot.form
        form.password = "hunter2"
        form.verify = "hunter2"
        controller.update(form)
        await controller.createAccount()
      }
      try await settle("the prepared helper was never read") { controller.snapshot.step == .switchToHelper }
      #expect(authorizations == 1)
      #expect(!commands.ran.contains(Action.closeLeftovers.arguments))
      #expect(controller.snapshot.helperReadied)
    }
  }

  @Test("a deleted account's open login comes first, and Close It closes it under the person's approval")
  func closeItClosesTheLogin() async throws {
    let commands = ScriptedCommands([
      "onboarding status": Self.leftoverMac, "onboarding close": #"{"ok": true, "closed": [502]}"#,
    ])
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      #expect(controller.snapshot.step == .closeLeftovers)
      #expect(controller.state.footer.next?.title == .closeIt)
      commands.answer("onboarding status", OnboardingReplies.finishedMac)
      await controller.closeLeftovers()
      #expect(commands.ran.contains(Action.closeLeftovers.arguments))
      try await settle("the closed login was never read") { controller.snapshot.facts.leftoverLogins.isEmpty }
      #expect(controller.snapshot.step == .closeLeftovers, "it says it closed until Continue")
      #expect(controller.state.footer.next?.title == .continue)
      guard case .screen(.closeLeftovers(let screen)) = controller.state.content else {
        Issue.record("the close screen shows")
        return
      }
      #expect(screen.isClosed)
      controller.send(.continueAfterClose)
      #expect(controller.snapshot.step == .agents, "setup goes on where it was")
    }
  }

  @Test("a cancelled approval keeps the close screen and says the login is still open")
  func aCancelledApprovalSaysItIsStillOpen() async throws {
    let refusal = #"{"ok": false, "code": "onboarding", "error": "It's still open. Close it before you switch."}"#
    let commands = ScriptedCommands(["onboarding status": Self.leftoverMac, "onboarding close": refusal])
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      await controller.closeLeftovers()
      #expect(controller.snapshot.step == .closeLeftovers)
      #expect(!controller.snapshot.choices.closedLeftovers)
      #expect(controller.state.note == .problem("It's still open. Close it before you switch."))
      #expect(controller.state.footer.next?.title == .closeIt, "Close It… can be tried again")
    }
  }

  @Test("Close It waits while it closes")
  func closeItWaitsWhileItCloses() async throws {
    let gate = ReleaseSignal()
    let commands = ScriptedCommands([
      "onboarding status": Self.leftoverMac, "onboarding close": #"{"ok": true, "closed": [502]}"#,
    ])
    commands.whileRunning { arguments in
      if arguments == Action.closeLeftovers.arguments { await gate.wait() }
    }
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      controller.send(.closeLeftovers)
      try await settle("Close It never ran") { controller.snapshot.working != nil }
      #expect(controller.state.note == .working(OnboardingWords.closing))
      #expect(controller.state.footer.next?.isEnabled == false)
      gate.open()
    }
  }

  @Test("giving up a helper whose login is still open says to log it out before deleting it")
  func givingUpALoggedInHelperSaysToLogItOut() async throws {
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.switchingMac,
      "onboarding forget": #"{"ok": true, "forgotten": "atlas", "loggedIn": true}"#,
    ])
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      await controller.chooseAnother()
      #expect(controller.snapshot.problem == OnboardingWords.logOutBeforeDeleting)
    }
  }
}
