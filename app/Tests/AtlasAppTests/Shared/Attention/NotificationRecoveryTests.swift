import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("NotificationRecovery")
struct NotificationRecoveryTests {
  /// What a notification's Recover did around its command.
  final class Seen {
    /// How often this app's control was ended first.
    var endedControl = 0
    /// How often the status was read again afterwards.
    var reads = 0
    /// How often Atlas was opened instead.
    var opened = 0
    /// The reasons told in a second notification, in order.
    var told: [String] = []
    /// Whether a window holds control.
    var holdingControl = false
  }

  /// A status whose last action's outcome is unknown, so Needs you holds and Recover restarts the helper.
  static let unknownOutcome: [String: Any] = [
    "ready": true, "canObserve": true, "overview": ["mode": "recovering", "state": "needsYou", "pending": 1],
  ]

  /// The helper's commands over a scripted service and command line, counting each time control is ended.
  ///
  /// - Parameters:
  ///   - service: The scripted service.
  ///   - commands: The scripted command line.
  ///   - seen: Where it counts.
  /// - Returns: The commands.
  private static func actions(_ service: ScriptedService, _ commands: ScriptedCommands, _ seen: Seen) -> HelperActions {
    HelperActions(
      console: SessionConsole(read: { true }),
      commands: commands,
      service: service,
      barrier: ControlBarrier(service: service, clock: ManualClock())
    ) { seen.endedControl += 1 }
  }

  /// A notification's Recover over `actions` and `service`, recording into `seen`.
  ///
  /// - Parameters:
  ///   - actions: The helper's commands.
  ///   - service: The scripted service it reads the status from.
  ///   - seen: Where it records.
  /// - Returns: The recovery.
  private static func recovery(
    _ actions: HelperActions,
    _ service: ScriptedService,
    _ seen: Seen
  ) -> NotificationRecovery {
    NotificationRecovery(
      actions: actions,
      status: { try? await service.status() },
      holdingControl: { seen.holdingControl },
      readStatus: { seen.reads += 1 },
      open: { seen.opened += 1 },
      tell: { seen.told.append($0) }
    )
  }

  @Test("the attention notification's Recover runs the same recovery as the alert")
  func runsTheAlertsRecovery() async throws {
    let service = ScriptedService()
    service.reply = Self.unknownOutcome
    let commands = ScriptedCommands(["restart": #"{"ok": true}"#])
    let seen = Seen()
    await Self.recovery(Self.actions(service, commands, seen), service, seen)
      .run()
    // The alert window's Recover on the same helper, through its own controller.
    let alertService = ScriptedService()
    alertService.reply = Self.unknownOutcome
    let alertCommands = ScriptedCommands(["restart": #"{"ok": true}"#])
    let alertSeen = Seen()
    let clock = ManualClock()
    let alert = AlertController(
      alert: .recover,
      actions: Self.actions(alertService, alertCommands, alertSeen),
      links: AlertLinks(clock: clock)
    )
    let confirming = Task { await alert.confirm() }
    let hold = Duration.seconds(Motion.Duration.doneHold.seconds)
    try await clock.untilSleeping(for: hold, "the alert's Recover never held on Recovered")
    clock.advance(by: hold)
    await confirming.value
    #expect(commands.ran == alertCommands.ran, "the same command, chosen from a status read as it runs")
    #expect(commands.ran == [Action.restart.arguments])
    #expect(seen.endedControl == alertSeen.endedControl, "this app's control ends first, as for the alert")
    #expect(seen.endedControl == 1)
    #expect(seen.reads == 1, "the status is read again once it ends")
    #expect(seen.opened == 0 && seen.told.isEmpty, "a recovery that worked says nothing more")
  }

  @Test("a Recover from a notification Needs you no longer holds opens Atlas and runs nothing")
  func aStaleNotificationOpensAtlas() async {
    let service = ScriptedService()
    service.reply = ["ready": true, "canObserve": true, "overview": ["mode": "observing", "state": "idle"]]
    let commands = ScriptedCommands(["restart": #"{"ok": true}"#, "start-input": #"{"ok": true}"#])
    let seen = Seen()
    await Self.recovery(Self.actions(service, commands, seen), service, seen)
      .run()
    #expect(commands.ran.isEmpty)
    #expect(seen.endedControl == 0, "control is left alone")
    #expect(seen.opened == 1)
  }

  @Test("a Recover from a notification while a window holds control opens Atlas and runs nothing")
  func holdingControlOpensAtlas() async {
    let service = ScriptedService()
    service.reply = Self.unknownOutcome
    let commands = ScriptedCommands(["restart": #"{"ok": true}"#])
    let seen = Seen()
    seen.holdingControl = true
    await Self.recovery(Self.actions(service, commands, seen), service, seen)
      .run()
    #expect(commands.ran.isEmpty)
    #expect(seen.endedControl == 0, "the window keeps its control")
    #expect(seen.opened == 1)
  }

  @Test("a blocked helper Recover fixes still counts as Needs you; one it cannot fix does not")
  func blockedNeedsRecoverable() throws {
    var overview = ViewerStatus.Overview()
    overview.mode = .blocked
    overview.recoverable = true
    #expect(NotificationRecovery.needsYou(overview))
    overview.recoverable = false
    #expect(!NotificationRecovery.needsYou(overview))
  }

  @Test("a recovery started from a notification that fails says why in a second notification")
  func aFailedRecoverySaysWhy() async {
    let service = ScriptedService()
    service.reply = Self.unknownOutcome
    let commands = ScriptedCommands(["restart": #"{"ok": false, "code": "busy", "error": "busy"}"#])
    let seen = Seen()
    await Self.recovery(Self.actions(service, commands, seen), service, seen)
      .run()
    #expect(seen.told == [HelperWords.recoverBusy])
  }

  @Test("a notification's Recover while another window's runs starts no second one, and says to wait")
  func noSecondRecovery() async throws {
    let service = ScriptedService()
    service.reply = Self.unknownOutcome
    let commands = ScriptedCommands(["restart": #"{"ok": true}"#])
    let gate = ReleaseSignal()
    commands.whileRunning { _ in await gate.wait() }
    let seen = Seen()
    let actions = Self.actions(service, commands, seen)
    // The alert's Recover, under way.
    let alert = Task { await actions.recover() }
    try await settle("the alert's Recover never ran") { commands.ran.count == 1 }
    await Self.recovery(actions, service, seen)
      .run()
    #expect(seen.told == [HelperWords.waitForAction])
    gate.open()
    _ = await alert.value
    #expect(commands.ran == [Action.restart.arguments], "one recovery ran")
  }
}
