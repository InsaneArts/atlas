import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasService
import AtlasServiceTesting
import Testing

@testable import AtlasApp

@Suite("HelperActions")
struct HelperActionsTests {
  @Test("a switch refuses when the graphical session leaves the console while control cleanup is pending")
  func switchingRechecksAfterCleanup() async throws {
    var onConsole = true
    let console = SessionConsole(read: { onConsole })
    let service = ScriptedService()
    let commands = ScriptedCommands([:])
    let barrier = ControlBarrier(service: service, clock: ManualClock())
    let actions = HelperActions(console: console, commands: commands, service: service, barrier: barrier)
    let gate = ReleaseSignal()
    let cleanup = Task { await barrier.tracking { await gate.wait() } }
    try await settle("cleanup never started") { barrier.isBusy }
    let switching = Task { await actions.switchTo("helper") }
    try await settle("the switch never waited for cleanup") { barrier.waiters == 1 }
    onConsole = false
    gate.open()
    await cleanup.value
    let refused = await switching.value
    #expect(refused?.ok == false)
    #expect(refused?.message == SessionConsole.refusal)
    #expect(commands.ran.isEmpty)
    #expect(!console.isOnConsole)
    #expect(await actions.switchTo("helper")?.ok == false)
    #expect(commands.ran.isEmpty)
  }

  /// A status whose last action's outcome is unknown, so Recover restarts the helper.
  static let unknownOutcome: [String: Any] = [
    "ready": true, "canObserve": true, "overview": ["mode": "recovering", "state": "needsYou", "pending": 1],
  ]

  /// The helper's commands over a scripted service and command line.
  ///
  /// - Parameters:
  ///   - service: The scripted service.
  ///   - commands: The scripted command line.
  /// - Returns: The commands.
  private static func actions(_ service: ScriptedService, _ commands: ScriptedCommands) -> HelperActions {
    HelperActions(
      console: SessionConsole(read: { true }),
      commands: commands,
      service: service,
      barrier: ControlBarrier(service: service, clock: ManualClock())
    )
  }

  @Test("Recover and Restart are one reservation, so a restart asked from another window while one runs never starts")
  func recoverAndRestartShareOneReservation() async throws {
    let commands = ScriptedCommands(["restart": #"{"ok": true}"#])
    let gate = ReleaseSignal()
    commands.whileRunning { _ in await gate.wait() }
    let service = ScriptedService()
    service.reply = Self.unknownOutcome
    let actions = Self.actions(service, commands)
    // Settings' Recover, restarting over an unknown outcome.
    let recovering = Task { await actions.recover() }
    try await settle("Recover never ran") { commands.ran.count == 1 }
    // The main window's Restart, asked while it runs.
    #expect(await actions.restart() == nil)
    gate.open()
    try await settle("Recover never ended") { !actions.isBusy }
    #expect(await recovering.value?.outcome.ok == true)
    #expect(commands.ran == [Action.restart.arguments])
  }

  @Test("Recover chooses between restarting and starting input from a status read once it runs, not an older one")
  func recoverChoosesFromAFreshStatus() async {
    let commands = ScriptedCommands(["restart": #"{"ok": true}"#, "start-input": #"{"ok": true}"#])
    let service = ScriptedService()
    // The window last read a quiet helper; the outcome became unknown since.
    service.reply = Self.unknownOutcome
    let recovery = await Self.actions(service, commands)
      .recover()
    #expect(recovery?.action == .restart)
    #expect(commands.ran == [Action.restart.arguments])
    #expect(service.statusCalls == 1, "it read the status itself")
  }

  @Test("a successful Restart the Helper… settles what an uncertain input start left, so the next Recover starts input")
  func aRestartClearsTheNextRestart() async {
    let commands = ScriptedCommands([
      "start-input": #"{"ok": false, "code": "uncertain", "error": "Run 'atlas restart'."}"#,
      "restart": #"{"ok": true}"#,
    ])
    let service = ScriptedService()
    service.reply = ["ready": true, "overview": ["mode": "blocked", "recoverable": true]]
    let actions = Self.actions(service, commands)
    #expect(await actions.recover()?.action == .startInput)
    #expect(await actions.restart()?.ok == true)
    commands.answer("start-input", #"{"ok": true}"#)
    #expect(await actions.recover()?.action == .startInput, "the restart already settled the unknown outcome")
    #expect(commands.ran == [Action.startInput.arguments, Action.restart.arguments, Action.startInput.arguments])
  }

  @Test("Resume waits while a Stop from any window is unanswered, and a Stop never waits")
  func resumeWaitsForAStop() async throws {
    let service = ScriptedService()
    let actions = Self.actions(service, ScriptedCommands([:]))
    service.hold = ["hold"]
    let stopping = Task { try await actions.setStopped(true) }
    try await settle("the Stop was never sent") { service.held["hold"] != nil }
    await #expect(throws: ServiceError(HelperWords.waitForAction)) { try await actions.setStopped(false) }
    #expect(service.log == ["stop"], "the Resume was not sent")
    service.resume("hold")
    try await settle("the Stop was never answered") { actions.agentRequests == 0 }
    try await stopping.value
    try await actions.setStopped(false)
    #expect(service.log == ["stop", "resume"])
  }
}
