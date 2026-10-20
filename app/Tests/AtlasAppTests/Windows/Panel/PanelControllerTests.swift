import AtlasKit
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@MainActor
@Suite("PanelController")
struct PanelControllerTests {
  /// A panel over scripted parts that opens no window.
  ///
  /// - Parameters:
  ///   - service: The scripted service.
  ///   - commands: The scripted command line, which by default knows no command.
  ///   - loginItem: The scripted login item.
  ///   - quit: Stands in for quitting Atlas.
  /// - Returns: The panel's controller.
  private static func panel(
    _ service: ScriptedService = ScriptedService(),
    commands: ScriptedCommands = ScriptedCommands([:]),
    loginItem: ScriptedLoginItem = ScriptedLoginItem(),
    quit: @escaping @MainActor () -> Void = {}
  ) -> PanelController {
    PanelController(
      service: service,
      commands: commands,
      loginItem: loginItem.seam,
      gettingStarted: {},
      openViewer: {},
      quit: quit
    )
  }

  @Test("the panel shows the service's status, and no picture when capture fails")
  func showsTheServiceStatus() async {
    let service = ScriptedService()
    service.reply = ["overview": ["mode": "recovering", "account": "atlas"]]
    service.fail = ["frame"]
    let controller = Self.panel(service)
    await controller.readStatus()
    await controller.readFrame()
    #expect(controller.status.overview.mode == .recovering)
    #expect(controller.picture == nil)
    #expect(controller.placeholder == PanelWords.pictureUnavailable)
  }

  @Test("only the service's answer is handed to the viewer: none before the first reading, none after a failed one")
  func onlyAnAnswerIsHandedOn() async {
    let service = ScriptedService()
    // Every field at its default, as a failed reading also shows: an answer all the same.
    service.reply = [:]
    let controller = Self.panel(service)
    #expect(controller.answer == nil && controller.answered == nil)
    await controller.readStatus()
    #expect(controller.answer == ViewerStatus())
    service.fail = ["status"]
    await controller.readStatus()
    #expect(controller.answer == nil && controller.answered == false)
  }

  @Test("revoking control ends every viewer's and says so")
  func revokingControl() async {
    let service = ScriptedService()
    let controller = Self.panel(service)
    await controller.revokeControl()
    #expect(service.revoked)
    #expect(controller.feedback == PanelWords.released)
    #expect(!controller.working)
  }

  @Test("the helper to switch to is the account atlas reports")
  func theHelperIsTheOneAtlasReports() async {
    let commands = ScriptedCommands([
      "switch --list": #"{"ok": true, "accounts": [{"name": "me"}, {"name": "atlas", "helper": true}]}"#,
      "switch": #"{"ok": true}"#,
    ])
    let controller = Self.panel(commands: commands)
    await controller.findHelper()
    #expect(controller.helper == "atlas")
    await controller.switchToHelper()
    #expect(commands.ran.last == Action.switchTo("atlas").arguments)
    #expect(controller.feedback == PanelWords.switched)
  }

  @Test("logging in locks the screen for the helper and says what to do next")
  func loggingInLocksTheScreen() async {
    let service = ScriptedService()
    service.reply = ["overview": ["mode": "loggedOut", "account": "atlas", "accountName": "Atlas"]]
    let commands = ScriptedCommands([
      "switch --list": #"{"ok": true, "accounts": [{"name": "atlas", "helper": true}]}"#,
      "switch": #"{"ok": true}"#,
    ])
    let controller = Self.panel(service, commands: commands)
    await controller.readStatus()
    await controller.logIn()
    #expect(commands.ran == [Action.listAccounts.arguments, Action.switchTo("atlas").arguments])
    #expect(controller.feedback == PanelWords.logInThenSwitchBack(to: "Atlas"))
  }

  @Test("logging in with no helper account found says where to look")
  func logInWithNoHelperSaysWhereToLook() async {
    let controller = Self.panel()
    await controller.logIn()
    #expect(controller.feedback == PanelWords.helperNotFound)
  }

  @Test("logging out asks first, ends control first, and offers to force it when an app holds it up")
  func loggingOutAsksFirstAndOffersForce() async {
    let service = ScriptedService()
    service.reply = ["ready": true, "overview": ["mode": "controlling", "accountName": "Atlas"]]
    let commands = ScriptedCommands([
      "logout": #"{"ok": false, "code": "logout-held", "error": "The helper has not logged out yet"}"#
    ])
    let controller = Self.panel(service, commands: commands)
    await controller.readStatus()
    controller.send(.askToLogOut)
    #expect(controller.logOutStage == .confirming)
    #expect(commands.ran.isEmpty, "nothing runs before the person confirms")
    await controller.logOut()
    #expect(service.log.contains("revoke"), "control ends first")
    #expect(commands.ran.last == Action.logOut(force: false).arguments)
    #expect(controller.feedback == "The helper has not logged out yet")
    #expect(controller.logOutStage == .held)
    // The held log-out stays pending; logging out anyway still may run.
    service.reply = ["overview": ["mode": "recovering"]]
    await controller.readStatus()
    #expect(controller.logOutStage == .held)
    commands.answer("logout", #"{"ok": true, "loggedOut": true}"#)
    await controller.logOut(force: true)
    #expect(commands.ran.last == Action.logOut(force: true).arguments)
    #expect(controller.logOutStage == .idle)
    #expect(controller.feedback == PanelWords.helperLoggedOut)
  }

  @Test("ending control before a log-out counts as running, and a log-out asked for meanwhile says why it waits")
  func theRevokeBeforeALogOutRuns() async throws {
    let service = ScriptedService()
    service.reply = ["ready": true, "overview": ["mode": "controlling"]]
    let commands = ScriptedCommands(["logout": #"{"ok": true}"#])
    let controller = Self.panel(service, commands: commands)
    await controller.readStatus()
    service.hold = ["revoke"]
    let first = Task { await controller.logOut() }
    try await settle("control was never asked to end") { service.held["revoke"] != nil }
    #expect(controller.working)
    await controller.logOut()
    #expect(controller.feedback == PanelWords.waitForAction)
    service.resume("revoke")
    await first.value
    #expect(commands.ran == [Action.logOut(force: false).arguments], "the first log-out ran, once")
  }

  @Test("a refused revoke stops the log-out, saying why")
  func aRefusedRevokeStopsTheLogOut() async {
    let service = ScriptedService()
    service.reply = ["ready": true, "overview": ["mode": "controlling"]]
    service.fail = ["revoke"]
    let commands = ScriptedCommands(["logout": #"{"ok": true}"#])
    let controller = Self.panel(service, commands: commands)
    await controller.readStatus()
    await controller.logOut()
    #expect(commands.ran.isEmpty)
    #expect(controller.feedback == "revoke failed")
    #expect(!controller.working)
  }

  @Test("a log-out the helper's mode no longer allows says so and runs nothing")
  func aLogOutTheModeForbidsSaysSo() async {
    let service = ScriptedService()
    service.reply = ["ready": true, "overview": ["mode": "agent"]]
    let commands = ScriptedCommands(["logout": #"{"ok": true}"#])
    let controller = Self.panel(service, commands: commands)
    await controller.readStatus()
    await controller.logOut()
    #expect(commands.ran.isEmpty)
    #expect(controller.feedback == PanelWords.cannotLogOut)
  }

  @Test("a confirmation the person reopens while a log-out runs stays once it ends")
  func aReopenedConfirmationStays() async {
    let service = ScriptedService()
    service.reply = ["ready": true, "overview": ["mode": "observing"]]
    let commands = ScriptedCommands([
      "logout": #"{"ok": false, "code": "logout-held", "error": "The helper has not logged out yet"}"#
    ])
    let controller = Self.panel(service, commands: commands)
    await controller.readStatus()
    commands.whileRunning { _ in controller.askToLogOut() }
    await controller.logOut()
    #expect(controller.logOutStage == .confirming)
  }

  @Test("a confirmation the mode no longer allows is dropped")
  func aConfirmationThatNoLongerAppliesIsDropped() async {
    let service = ScriptedService()
    let controller = Self.panel(service)
    await controller.readStatus()
    controller.send(.askToLogOut)
    service.reply = ["overview": ["mode": "agent"]]
    await controller.readStatus()
    #expect(controller.logOutStage == .idle, "an agent started meanwhile")
  }

  @Test("a message said while a command runs never ends it: the panel stays working, its spinner on, until it ends")
  func aMessageNeverEndsARunningCommand() async {
    let service = ScriptedService()
    let commands = ScriptedCommands(["restart": #"{"ok": true}"#])
    let loginItem = ScriptedLoginItem(available: true)
    loginItem.refusal = ServiceError("Not allowed")
    let controller = Self.panel(service, commands: commands, loginItem: loginItem)
    let seen = PanelWorkObservation()
    commands.whileRunning { _ in
      await controller.revokeControl()
      seen.feedback = controller.state.feedback
      controller.setOpenAtLogin(true)
      seen.working = controller.working
    }
    await controller.recover()
    #expect(seen.feedback == Feedback(text: PanelWords.released, isWorking: true), "Recover still ran")
    #expect(seen.working == true, "Recover still ran")
    #expect(!controller.working)
    #expect(controller.state.feedback?.isWorking == false)
  }

  @Test("Open at Login follows the system, and a refusal says why")
  func openAtLoginFollowsTheSystem() {
    let loginItem = ScriptedLoginItem(available: true)
    let controller = Self.panel(loginItem: loginItem)
    controller.setOpenAtLogin(true)
    #expect(controller.opensAtLogin == OpenAtLogin.State(isAvailable: true, isEnabled: true))
    loginItem.refusal = ServiceError("Not allowed")
    controller.setOpenAtLogin(false)
    #expect(controller.opensAtLogin.isEnabled, "the system kept it on")
    #expect(controller.feedback == "Not allowed")
  }

  @Test("each opening reads Open at Login again, so a change made elsewhere meanwhile shows in its switch")
  func openAtLoginIsReadAgainOnOpening() {
    let loginItem = ScriptedLoginItem(available: true, enabled: true)
    let controller = Self.panel(loginItem: loginItem)
    controller.watch(open: true)
    #expect(controller.state.menu.opensAtLogin == true)
    controller.watch(open: false)
    // Getting Started turns it off while the panel is closed.
    loginItem.enabled = false
    controller.watch(open: true)
    #expect(controller.state.menu.opensAtLogin == false)
  }

  @Test("Quit Atlas in the panel quits")
  func quitInThePanelQuits() {
    let quits = CallCount()
    let controller = Self.panel(quit: { quits.count += 1 })
    controller.send(.quit)
    #expect(quits.count == 1)
  }
}
