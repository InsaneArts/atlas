import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Observation
import Testing

@testable import AtlasApp

@Suite("AlertController")
struct AlertControllerTests {
  /// What a test's stand-in links saw: the overview they give, and how often the window closed; observable, so a
  /// test's `settle` hears each read.
  @Observable
  final class Seen {
    /// The overview the links give.
    var overview = ViewerStatus.Overview()
    /// What reading the status again turns the overview into; nil keeps it.
    var nextOverview: ViewerStatus.Overview?
    /// How often the window closed.
    var closed = 0
    /// How often the status was read again.
    var reads = 0
    /// The read, counted from one, from which reading the status waits for ``gate``; nil never waits.
    var holdFrom: Int?
    /// What a held read waits for.
    let gate = ReleaseSignal()
  }

  /// An alert window's controller over scripted commands and a stand-in for the rest of the app.
  ///
  /// - Parameters:
  ///   - alert: Which alert shows.
  ///   - service: The scripted service.
  ///   - commands: The scripted command line.
  ///   - seen: What the stand-in links give and record.
  /// - Returns: The controller.
  private static func alert(
    _ alert: HelperAlert,
    service: ScriptedService = ScriptedService(),
    commands: ScriptedCommands,
    seen: Seen
  ) -> AlertController {
    AlertController(
      alert: alert,
      actions: HelperActions(
        commands: commands,
        service: service,
        barrier: ControlBarrier(service: service, clock: ManualClock())
      ),
      links: AlertLinks(
        overview: { seen.overview },
        readStatus: {
          seen.reads += 1
          if let holdFrom = seen.holdFrom, seen.reads >= holdFrom { await seen.gate.wait() }
          if let next = seen.nextOverview { seen.overview = next }
        },
        close: { _ in seen.closed += 1 },
        clock: ManualClock()
      )
    )
  }

  /// An overview in `mode`, with `pending` actions of unknown outcome.
  ///
  /// - Parameters:
  ///   - mode: The helper's mode.
  ///   - pending: Actions of unknown outcome.
  /// - Returns: The overview.
  private static func overview(_ mode: HelperMode, pending: Int = 0) -> ViewerStatus.Overview {
    var overview = ViewerStatus.Overview()
    overview.mode = mode
    overview.pending = pending
    return overview
  }

  @Test("Recover reads the status once it runs, restarts a helper whose outcome is unknown, and closes once it worked")
  func recoverRestartsAnUnknownOutcome() async {
    let commands = ScriptedCommands(["start-input": #"{"ok": true}"#, "restart": #"{"ok": true}"#])
    let seen = Seen()
    seen.overview = Self.overview(.blocked)
    let service = ScriptedService()
    // The outcome became unknown since the panel last read the status.
    service.reply = ["ready": true, "overview": ["mode": "recovering", "pending": 1]]
    let controller = Self.alert(.recover, service: service, commands: commands, seen: seen)
    await controller.confirm()
    #expect(commands.ran == [Action.restart.arguments])
    #expect(seen.closed == 1)
  }

  @Test("the alert's command runs once however many times it is pressed")
  func theCommandRunsOnce() async {
    let commands = ScriptedCommands(["start-input": #"{"ok": true}"#])
    let seen = Seen()
    let controller = Self.alert(.recover, commands: commands, seen: seen)
    commands.whileRunning { _ in await controller.confirm() }
    await controller.confirm()
    #expect(commands.ran == [Action.startInput.arguments])
  }

  @Test(
    "while the status is read after a command, the alert stays busy: no second run, no way out, no other alert"
  )
  func theAlertStaysBusyThroughItsRefresh() async throws {
    let commands = ScriptedCommands(["start-input": #"{"ok": true}"#, "logout": #"{"ok": true}"#])
    let seen = Seen()
    // The window reads the status again after its command; that read is held.
    seen.holdFrom = 1
    let controller = Self.alert(.recover, commands: commands, seen: seen)
    let confirming = Task { await controller.confirm() }
    try await settle("the status was never read after the command") { seen.reads == 1 }
    #expect(controller.state.isBusy)
    await controller.confirm()
    controller.cancel()
    controller.show(.logOut)
    #expect(commands.ran == [Action.startInput.arguments], "the command ran once")
    #expect(seen.closed == 0, "the way out waits")
    #expect(controller.alert == .recover, "no other alert takes its place")
    seen.gate.open()
    await confirming.value
    #expect(seen.closed == 1, "the alert it started from closes once it worked")
    #expect(!controller.state.isBusy)
  }

  @Test(
    "a Recover the command line refuses keeps the alert, saying why in plain words, never naming a command",
    arguments: [("uncertain", HelperWords.recoverNeedsRestart), ("busy", HelperWords.recoverBusy)]
  )
  func aRefusedRecoverSaysWhyPlainly(code: String, words: String) async {
    let commands = ScriptedCommands([
      "start-input": #"{"ok": false, "code": "\#(code)", "error": "Run 'atlas restart'."}"#
    ])
    let seen = Seen()
    let controller = Self.alert(.recover, commands: commands, seen: seen)
    await controller.confirm()
    #expect(controller.state.message == words)
    #expect(seen.closed == 0)
  }

  @Test("a Recover refused over an unknown outcome makes the next one a restart, though the status says nothing new")
  func anUnknownOutcomeMakesTheNextRecoverARestart() async {
    let commands = ScriptedCommands([
      "start-input": #"{"ok": false, "code": "uncertain", "error": "Run 'atlas restart'."}"#,
      "restart": #"{"ok": true}"#,
    ])
    let controller = Self.alert(.recover, commands: commands, seen: Seen())
    await controller.confirm()
    await controller.confirm()
    #expect(commands.ran == [Action.startInput.arguments, Action.restart.arguments])
  }

  @Test("logging out ends control first, and asks again with Log Out Anyway when an app holds it up")
  func loggingOutOffersForce() async {
    let service = ScriptedService()
    let commands = ScriptedCommands([
      "logout": #"{"ok": false, "code": "logout-held", "error": "The helper has not logged out yet"}"#
    ])
    let seen = Seen()
    seen.overview = Self.overview(.controlling)
    let controller = Self.alert(.logOut, service: service, commands: commands, seen: seen)
    await controller.confirm()
    #expect(service.revoked, "control ends first")
    #expect(controller.alert == .logOutAnyway)
    #expect(seen.closed == 0)
    commands.answer("logout", #"{"ok": true, "loggedOut": true}"#)
    seen.overview = Self.overview(.recovering)
    await controller.confirm()
    #expect(commands.ran.last == Action.logOut(force: true).arguments)
    #expect(seen.closed == 1)
  }

  @Test("a refused revoke stops the log-out and keeps the alert, saying why")
  func aRefusedRevokeStopsTheLogOut() async {
    let service = ScriptedService()
    service.fail = ["revoke"]
    let commands = ScriptedCommands(["logout": #"{"ok": true}"#])
    let seen = Seen()
    seen.overview = Self.overview(.controlling)
    let controller = Self.alert(.logOut, service: service, commands: commands, seen: seen)
    await controller.confirm()
    #expect(commands.ran.isEmpty)
    #expect(controller.state.message == "revoke failed.")
    #expect(!controller.state.isBusy)
  }

  @Test("a log-out the helper's mode no longer allows says so and runs nothing")
  func aLogOutTheModeForbidsSaysSo() async {
    let commands = ScriptedCommands(["logout": #"{"ok": true}"#])
    let seen = Seen()
    seen.overview = Self.overview(.agent)
    let controller = Self.alert(.logOut, commands: commands, seen: seen)
    await controller.confirm()
    #expect(commands.ran.isEmpty)
    #expect(controller.state.message == AlertWords.cannotLogOut)
  }
}
