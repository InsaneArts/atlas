import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("ViewerController+Alert")
struct ViewerControllerAlertTests {
  /// How long a done Recover holds on Recovered.
  private static let hold = Duration.seconds(Motion.Duration.doneHold.seconds)

  /// A service whose helper needs recovering after an action of unknown outcome.
  ///
  /// - Returns: The service.
  private static func needsYou() -> ScriptedService {
    let service = ScriptedService()
    service.reply = [
      "ready": true, "canObserve": true, "overview": ["mode": "recovering", "state": "needsYou", "pending": 1],
    ]
    return service
  }

  @Test("the alert's Recover runs once however many times it is pressed")
  func recoverRunsOnce() async throws {
    let commands = ScriptedCommands(["restart": #"{"ok": true}"#])
    let release = ReleaseSignal()
    let running = CallCount()
    commands.whileRunning { _ in
      running.count += 1
      await release.wait()
    }
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(Self.needsYou(), commands: commands, clock: clock)
    try #require(viewer.state.alert != nil, "the recovery alert shows")
    viewer.send(.alertConfirm)
    viewer.send(.alertConfirm)
    try await settle("Recover never ran") { running.count == 1 }
    viewer.send(.alertConfirm)
    #expect(viewer.state.alert?.isBusy == true, "its buttons wait while it runs")
    // The third press reaches the controller while the command still waits, before it is let go.
    await afterQueuedWork()
    release.open()
    try await settle("Recover never held on Recovered") { viewer.session.recoveryDone }
    clock.advance(by: Self.hold)
    try await settle("Recover never ended") { !viewer.session.recovering }
    #expect(commands.ran == [Action.restart.arguments])
    viewer.close()
  }

  @Test("a successful recovery shows Recovered in the main window and closes once the hold ends")
  func aSuccessfulRecoveryHoldsThenCloses() async throws {
    let commands = ScriptedCommands(["restart": #"{"ok": true}"#])
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(Self.needsYou(), commands: commands, clock: clock)
    viewer.send(.alertConfirm)
    try await settle("Recover never held on Recovered") { viewer.session.recoveryDone }
    #expect(viewer.state.alert?.phase == .done(title: HelperWords.recovered))
    viewer.send(.alertCancel)
    #expect(viewer.state.alert != nil, "nothing dismisses it while it holds")
    clock.advance(by: Self.hold)
    try await settle("the alert never closed") { !viewer.session.recovering }
    #expect(viewer.session.alert == nil)
    #expect(viewer.session.notice == Words.recovered)
    viewer.close()
  }

  @Test("a refused recovery keeps the main window's alert, saying why, with its buttons back")
  func aRefusedRecoveryStays() async throws {
    let commands = ScriptedCommands(["restart": #"{"ok": false, "code": "busy", "error": "busy"}"#])
    let viewer = try await ViewerController.watching(Self.needsYou(), commands: commands)
    viewer.send(.alertConfirm)
    try await settle("Recover never ended") { commands.ran.count == 1 && !viewer.session.recovering }
    #expect(viewer.state.alert?.phase == .refused(message: HelperWords.recoverBusy))
    viewer.send(.alertCancel)
    #expect(viewer.session.recoveryRefusal == nil, "Not Now clears it")
    viewer.close()
  }

  @Test("a refusal is forgotten once nothing needs recovery, so it never shows on a later alert")
  func aRefusalClearsWithItsNeed() async throws {
    let service = Self.needsYou()
    let commands = ScriptedCommands(["restart": #"{"ok": false, "code": "busy", "error": "busy"}"#])
    let viewer = try await ViewerController.watching(service, commands: commands)
    viewer.send(.alertConfirm)
    try await settle("the refusal never showed") { viewer.session.recoveryRefusal != nil }
    service.reply = ["ready": true, "canObserve": true, "overview": ["mode": "observing", "state": "idle"]]
    await viewer.readingLoop.readStatus()
    #expect(viewer.session.recoveryRefusal == nil)
    viewer.close()
  }

  @Test("an alert asked while Recovered holds leaves its wait notice in place after the hold")
  func aWaitNoticeOutlastsTheHold() async throws {
    let commands = ScriptedCommands(["restart": #"{"ok": true}"#])
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(Self.needsYou(), commands: commands, clock: clock)
    viewer.send(.alertConfirm)
    try await settle("Recover never held on Recovered") { viewer.session.recoveryDone }
    viewer.ask(.logOut)
    #expect(viewer.session.notice == HelperWords.waitForAction)
    clock.advance(by: Self.hold)
    try await settle("the alert never closed") { !viewer.session.recovering }
    #expect(viewer.session.notice == HelperWords.waitForAction, "the wait notice is not overwritten")
    viewer.close()
  }
}
