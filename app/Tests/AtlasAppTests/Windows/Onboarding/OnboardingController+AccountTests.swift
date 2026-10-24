import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("OnboardingController account")
struct OnboardingControllerAccountTests {
  @Test("a fresh Mac with no candidates picks a new account, and Continue opens the form")
  func aFreshMacPicksANewAccount() async throws {
    let controller = OnboardingController.scripted(ScriptedCommands(["onboarding status": OnboardingReplies.freshMac]))
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      #expect(controller.snapshot.step == .welcome)
      #expect(controller.snapshot.picked == .newAccount, "no candidates: a new account is picked")
      controller.send(.begin)
      await controller.useHelper()
      #expect(controller.snapshot.step == .createAccount)
    }
  }

  @Test("Back leaves the form for the account choice, and the choice for the welcome")
  func backStepsBack() async throws {
    let controller = OnboardingController.scripted(ScriptedCommands(["onboarding status": OnboardingReplies.freshMac]))
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      controller.send(.begin)
      await controller.useHelper()
      controller.send(.back)
      #expect(controller.snapshot.step == .chooseHelper)
      controller.send(.back)
      #expect(controller.snapshot.step == .welcome)
    }
  }

  @Test("a helper macOS could not ready says to open Atlas there, and one it readied says Atlas opens by itself")
  func anUnreadiedHelperSaysToOpenAtlas() async throws {
    let unreadied = ScriptedCommands([
      "onboarding status": OnboardingReplies.freshMac, "onboarding create": #"{"ok": true, "readied": false}"#,
    ])
    let controller = OnboardingController.scripted(unreadied)
    var form = controller.snapshot.form
    form.password = "hunter2"
    form.verify = "hunter2"
    controller.update(form)
    await controller.createAccount()
    #expect(!controller.snapshot.helperReadied)
    #expect(!OnboardingPresenter.switchToHelper(controller.snapshot).opensByItself)

    let readied = ScriptedCommands([
      "onboarding status": OnboardingReplies.freshMac, "onboarding create": #"{"ok": true, "readied": true}"#,
    ])
    let other = OnboardingController.scripted(readied)
    other.update(form)
    await other.createAccount()
    #expect(OnboardingPresenter.switchToHelper(other.snapshot).opensByItself)
  }

  @Test("the password reaches macOS on standard input and the form forgets it")
  func thePasswordTravelsOnStandardInput() async throws {
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.freshMac, "onboarding create": #"{"ok": true}"#,
    ])
    let controller = OnboardingController.scripted(commands)
    var form = controller.snapshot.form
    form.password = "hunter2"
    form.verify = "hunter2"
    controller.update(form)
    await controller.createAccount()
    let create = try #require(commands.ran.firstIndex { $0.starts(with: ["onboarding", "create"]) })
    #expect(commands.ran[create] == Action.createAccount(account: form.accountName, fullName: form.fullName).arguments)
    #expect(commands.inputs[create] == Data("hunter2\n".utf8))
    #expect(controller.snapshot.form.password.isEmpty && controller.snapshot.form.verify.isEmpty)
  }

  @Test("a failed command shows its message in one line, and nothing runs any more")
  func aFailedCommandIsShown() async throws {
    let withQA = OnboardingReplies.freshMac
      .replacingOccurrences(
        of: #""candidates": []"#,
        with: #""candidates": [{"name": "qa"}]"#
      )
    let controller = OnboardingController.scripted(ScriptedCommands(["onboarding status": withQA]))
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      controller.send(.pick(.existing("qa")))
      await controller.useHelper()
      #expect(controller.snapshot.problem == "unknown command.", "a command's message reads as a sentence")
      #expect(controller.snapshot.working == nil)
    }
  }

  @Test("two quick presses of Create Account run the command once")
  func twoQuickPressesCreateOnce() async throws {
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.freshMac, "onboarding create": #"{"ok": true}"#,
      "onboarding forget": #"{"ok": true}"#,
    ])
    let controller = OnboardingController.scripted(commands)
    var form = controller.snapshot.form
    form.password = "hunter2"
    form.verify = "hunter2"
    controller.update(form)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      controller.send(.createAccount)
      controller.send(.createAccount)
      // The sentinel queues behind both presses: a second account made would come before it.
      controller.send(.chooseAnother)
      try await settle("the sentinel never ran") { commands.ran.contains(Action.forgetHelper.arguments) }
    }
    let creates = commands.ran.indices.filter { commands.ran[$0].starts(with: ["onboarding", "create"]) }
    #expect(creates.count == 1)
    #expect(
      creates.map { commands.inputs[$0] } == [Data("hunter2\n".utf8)],
      "the password went once, on standard input"
    )
  }
}
