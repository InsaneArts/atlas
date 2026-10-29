import AsyncSupportTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("ViewerPresenter+Cleanup")
struct ViewerPresenterCleanupTests {
  /// Claude Code's session key.
  static let key = "3f9a1c07b2e4"

  /// What the main window holds with Claude Code chosen and its record listed as `record`.
  ///
  /// - Parameter record: Claude Code's record.
  /// - Returns: The snapshot.
  /// - Throws: A decoding error, which a fixed reply never gives.
  static func chosen(_ record: HostSessionRecord) throws -> ViewerSnapshot {
    var snapshot = try ViewerSnapshot.listing(ViewerPresenterSessionsTests.three)
    snapshot.detailsShown = true
    snapshot.session.sessionChoice = .one(record.key)
    snapshot.panes.sessions.list = HostSessionList(sessions: [record])
    return snapshot
  }

  /// Claude Code's record, with what it left.
  ///
  /// - Parameters:
  ///   - key: Its key; Claude Code's by default.
  ///   - copies: The copies made for it; none by default.
  ///   - installs: The apps its tasks installed; none by default.
  ///   - cleanup: Where its newest unfinished cleanup stands; none by default.
  /// - Returns: The record.
  static func record(
    key: String = key,
    copies: [HostSessionCopy] = [],
    installs: [String] = [],
    cleanup: CleanupState? = nil
  ) -> HostSessionRecord {
    HostSessionRecord(
      key: key,
      name: "Claude Code · understudy",
      host: .claude,
      copies: copies,
      installs: installs,
      cleanup: cleanup
    )
  }

  /// A preview of quitting Probe and Notes, rolling back a plan, moving Probe to the helper's Trash and removing two copies.
  ///
  /// - Parameter active: Whether the session may still be working.
  /// - Returns: The preview.
  static func preview(active: Bool) -> CleanupReply {
    CleanupReply(
      ok: true,
      preview: true,
      steps: [
        CleanupStep(number: 1, kind: .quit, apps: ["Probe", "Notes"]),
        CleanupStep(number: 2, kind: .undoPlan),
        CleanupStep(number: 3, kind: .removeInstall, app: "Probe", unchanged: true),
        CleanupStep(number: 4, kind: .removeCopy, app: "Probe"),
        CleanupStep(number: 5, kind: .removeCopy),
      ],
      active: active
    )
  }

  @Test("the button shows only when the session left something")
  func theButtonShowsOnlyWhenTheSessionLeftSomething() throws {
    let cleanup = { (record: HostSessionRecord) in
      ViewerPresenter.details(try Self.chosen(record)).hostSession?.cleanup
    }
    #expect(try cleanup(Self.record()) == nil, "a session that left nothing has no button")
    #expect(try cleanup(Self.record(cleanup: .complete)) == nil, "a cleanup that finished leaves nothing")
    let copied = try #require(try cleanup(Self.record(copies: [HostSessionCopy(app: "Opie")])))
    #expect(copied.title == "Clean Up This Session's Work…")
    #expect(copied.session == Self.key && copied.stage == .ready && copied.isEnabled)
    #expect(try cleanup(Self.record(installs: ["Probe"])) != nil, "an app its task installed is left")
    #expect(try cleanup(Self.record(cleanup: .incomplete)) != nil, "a cleanup that did not finish is left")
    let terminal = Self.record(key: "", copies: [HostSessionCopy(app: "Opie")])
    #expect(try cleanup(terminal) == nil, "Terminal's session has no key a cleanup can name")
  }

  @Test("the confirmation lists what will be quit and moved to the Trash")
  func theConfirmationListsWhatWillBeQuitAndMovedToTheTrash() throws {
    let steps = ViewerPresenter.cleanupSteps(Self.preview(active: false))
    #expect(
      steps == [
        "Quits Probe and Notes",
        "Undoes 1 environment change",
        "Moves Probe to the helper's Trash",
        "Removes 2 copies",
      ]
    )
    var snapshot = try Self.chosen(Self.record(copies: [HostSessionCopy(app: "Probe")]))
    snapshot.session.alert = .cleanup(session: Self.key, steps: steps, warning: nil)
    let alert = try #require(ViewerPresenter.alert(snapshot))
    #expect(alert.title == "Clean up this session's work?")
    #expect(alert.message == steps.joined(separator: "\n"))
    #expect(alert.buttons.map(\.title) == ["Cancel", "Clean Up"])
    #expect(alert.buttons.map(\.role) == [.safeCancel, .destructive], "Cancel is the default, Clean Up red")
    let changed = CleanupReply(
      preview: true,
      steps: [
        CleanupStep(number: 1, kind: .removeInstall, app: "Probe", unchanged: false),
        CleanupStep(number: 2, kind: .removeCopy, state: .done),
      ]
    )
    #expect(
      ViewerPresenter.cleanupSteps(changed) == ["Keeps Probe, as it changed since it was installed"],
      "an app that changed is kept, and a step an earlier cleanup did is left out"
    )
    let plans = CleanupReply(
      preview: true,
      steps: [CleanupStep(number: 1, kind: .undoPlan), CleanupStep(number: 2, kind: .undoPlan)]
    )
    #expect(ViewerPresenter.cleanupSteps(plans) == ["Undoes 2 environment changes"])
    let one = CleanupReply(preview: true, steps: [CleanupStep(number: 1, kind: .removeCopy)])
    #expect(ViewerPresenter.cleanupSteps(one) == ["Removes 1 copy"])
  }

  @Test("an uncertain or blocked cleanup says so on its own")
  func anUncertainOrBlockedCleanupSaysSoOnItsOwn() {
    let uncertain = CleanupReply(
      state: .uncertain,
      steps: [CleanupStep(number: 1, kind: .quit, apps: ["Probe"], state: .uncertain)]
    )
    #expect(
      ViewerPresenter.cleanupNotice(uncertain)
        == "Atlas doesn't know whether the last step finished. Recover the helper first, then clean up again."
    )
    let blocked = CleanupReply(
      state: .blocked,
      steps: [CleanupStep(number: 1, kind: .removeCopy, state: .blocked, reason: "stopped")]
    )
    #expect(
      ViewerPresenter.cleanupNotice(blocked)
        == "Agents are stopped, so the cleanup stopped. Resume, then clean up again."
    )
    let failed = CleanupReply(state: .incomplete, steps: [CleanupStep(number: 1, kind: .removeCopy, state: .failed)])
    #expect(ViewerPresenter.cleanupNotice(failed) == "The cleanup didn't finish. Details lists each step.")
  }

  @Test("a live session's confirmation says it may still be working")
  func aLiveSessionsConfirmationSaysItMayStillBeWorking() throws {
    let live = Self.preview(active: true)
    let warning = try #require(ViewerPresenter.cleanupWarning(live))
    #expect(warning == "This session may still be working.")
    #expect(ViewerPresenter.cleanupWarning(Self.preview(active: false)) == nil)
    var snapshot = try Self.chosen(Self.record(copies: [HostSessionCopy(app: "Probe")]))
    snapshot.session.alert = .cleanup(session: Self.key, steps: ViewerPresenter.cleanupSteps(live), warning: warning)
    let message = try #require(ViewerPresenter.alert(snapshot)?.message)
    #expect(message.hasPrefix("Quits Probe and Notes\n"))
    #expect(message.hasSuffix("\n\nThis session may still be working."))
  }

  @Test("kept and failed steps read apart in the receipt")
  func keptAndFailedStepsReadApartInTheReceipt() throws {
    let applied = CleanupReply(
      state: .incomplete,
      steps: [
        CleanupStep(number: 1, kind: .quit, apps: ["Probe"], state: .done),
        CleanupStep(number: 2, kind: .removeInstall, app: "Probe", state: .kept, reason: "changed"),
        CleanupStep(number: 3, kind: .removeCopy, app: "Probe", state: .failed, reason: "in-use", retry: true),
        CleanupStep(number: 4, kind: .removeCopy, state: .failed, reason: "io"),
        CleanupStep(number: 5, kind: .removeCopy, app: "Notes", state: .done, alreadyGone: true),
        CleanupStep(number: 6, kind: .undoPlan, state: .uncertain),
        CleanupStep(number: 7, kind: .unknown, state: .pending),
        CleanupStep(number: 8, kind: .removeCopy, app: "Mail", state: .kept, reason: "other-volume"),
        CleanupStep(number: 9, kind: .removeInstall, app: "Mail", state: .kept, reason: "unreadable"),
      ]
    )
    #expect(
      ViewerPresenter.cleanupReceipt(applied) == [
        Fact(name: "Quit Probe", value: "Done"),
        Fact(name: "Remove Probe from Applications", value: "Kept: changed since it was installed"),
        Fact(name: "Remove the copy of Probe", value: "Couldn't: still in use. Try again."),
        Fact(name: "Remove a copy", value: "Couldn't: the file system refused"),
        Fact(name: "Remove the copy of Notes", value: "Done: it was already gone"),
        Fact(name: "Undo an environment change", value: "Not known whether it worked"),
        Fact(name: "Another step", value: "Didn't run"),
        Fact(name: "Remove the copy of Mail", value: "Kept: it could not be moved to the Trash"),
        Fact(name: "Remove Mail from Applications", value: "Kept: Atlas couldn't read its record of it"),
      ]
    )
    #expect(ViewerPresenter.cleanupNotice(applied) == "The cleanup didn't finish. Details lists each step.")
    #expect(ViewerPresenter.cleanupNotice(CleanupReply(ok: true, state: .complete)) == ViewerWords.nothingToCleanUp)
    let unknown = CleanupStep(number: 1, kind: .removeCopy, state: .kept, reason: "a-reason-from-later")
    #expect(ViewerPresenter.cleanupOutcome(unknown) == "Kept", "a reason this app does not know is never shown")
    var snapshot = try Self.chosen(Self.record(copies: [HostSessionCopy(app: "Probe")]))
    snapshot.session.cleanupReceipts[Self.key] = applied
    let cleanup = try #require(ViewerPresenter.details(snapshot).hostSession?.cleanup)
    #expect(cleanup.receipt.count == 9, "the receipt shows above the button, which can be pressed again")
    #expect(cleanup.isEnabled)
  }

  @Test("an install moved to the helper's Trash says so in the receipt and a copy reads as done")
  func anInstallMovedToTheHelpersTrashSaysSoInTheReceiptAndACopyReadsAsDone() {
    let applied = CleanupReply(
      ok: true,
      state: .complete,
      steps: [
        CleanupStep(number: 1, kind: .quit, apps: ["Probe"], state: .done),
        CleanupStep(number: 2, kind: .undoPlan, state: .done),
        CleanupStep(
          number: 3,
          kind: .removeInstall,
          app: "Probe",
          state: .done,
          trashed: "Probe.app (atlas a7d2c0ffee12)"
        ),
        CleanupStep(number: 4, kind: .removeCopy, app: "TextEdit", state: .done),
        CleanupStep(number: 5, kind: .removeCopy, app: "Notes", state: .done, alreadyGone: true),
      ]
    )
    #expect(
      ViewerPresenter.cleanupReceipt(applied) == [
        Fact(name: "Quit Probe", value: "Done"),
        Fact(name: "Undo an environment change", value: "Done"),
        Fact(name: "Remove Probe from Applications", value: "Moved to the helper's Trash"),
        Fact(name: "Remove the copy of TextEdit", value: "Done"),
        Fact(name: "Remove the copy of Notes", value: "Done: it was already gone"),
      ]
    )
    #expect(ViewerPresenter.cleanupNotice(applied) == "Cleaned up this session's work.")
  }

  @Test("the button stays disabled while a cleanup runs")
  func theButtonStaysDisabledWhileACleanupRuns() async throws {
    let service = ScriptedService()
    service.sessionList = HostSessionList(sessions: [Self.record(copies: [HostSessionCopy(app: "Probe")])])
    service.cleanupPreview = Self.preview(active: false)
    service.cleanupApplied = CleanupReply(
      ok: true,
      state: .complete,
      steps: [
        CleanupStep(number: 1, kind: .removeCopy, app: "Probe", state: .done)
      ]
    )
    let viewer = try await ViewerController.watching(service)
    viewer.send(.chooseSession(.one(Self.key)))
    try await settle("the list was never read") { viewer.panes.sessions.list != nil }
    let cleanup = { ViewerPresenter.details(viewer.snapshot).hostSession?.cleanup }
    #expect(cleanup()?.isEnabled == true)

    service.hold = ["cleanup.json"]
    viewer.send(.askCleanup(session: Self.key))
    try await settle("the preview was never asked for") { service.held["cleanup.json"] != nil }
    #expect(cleanup()?.stage == .working, "it waits while what it takes is read")
    viewer.send(.askCleanup(session: Self.key))
    await afterQueuedWork()
    service.resume("cleanup.json")
    try await settle("the alert never asked") { viewer.session.alert != nil }
    #expect(service.cleanups == ["preview \(Self.key)"], "a second press reads nothing more")
    #expect(cleanup()?.isEnabled == true, "it can be pressed once it is read")

    service.hold = ["cleanup"]
    viewer.send(.alertConfirm)
    viewer.send(.alertConfirm)
    try await settle("Clean Up never ran") { service.held["cleanup"] != nil }
    #expect(cleanup()?.isEnabled == false, "it waits while the cleanup runs")
    #expect(viewer.state.alert?.isBusy == true, "the alert's buttons wait too")
    viewer.send(.alertConfirm)
    await afterQueuedWork()
    service.resume("cleanup")
    try await settle("the cleanup never ended") { !viewer.session.alertRunning && viewer.session.alert == nil }
    #expect(service.cleanups == ["preview \(Self.key)", "apply \(Self.key)"], "Clean Up runs once")
    #expect(viewer.session.notice == "Cleaned up this session's work.")
    #expect(cleanup()?.receipt == [Fact(name: "Remove the copy of Probe", value: "Done")])
    #expect(cleanup()?.isEnabled == true)
    viewer.close()
  }

  @Test("a preview read after the person moved on asks nothing")
  func aPreviewReadAfterThePersonMovedOnAsksNothing() async throws {
    let service = ScriptedService()
    service.sessionList = HostSessionList(sessions: [Self.record(copies: [HostSessionCopy(app: "Probe")])])
    service.cleanupPreview = Self.preview(active: false)
    let viewer = try await ViewerController.watching(service)
    viewer.send(.chooseSession(.one(Self.key)))
    try await settle("the list was never read") { viewer.panes.sessions.list != nil }

    service.hold = ["cleanup.json"]
    viewer.send(.askCleanup(session: Self.key))
    try await settle("the preview was never asked for") { service.held["cleanup.json"] != nil }
    viewer.send(.chooseSession(.all))
    service.resume("cleanup.json")
    try await settle("the preview never came back") { viewer.session.cleaningUp == nil }
    #expect(viewer.session.alert == nil, "another session is shown, so nothing asks about this one")

    viewer.send(.chooseSession(.one(Self.key)))
    service.hold = ["cleanup.json"]
    viewer.send(.askCleanup(session: Self.key))
    try await settle("the second preview was never asked for") { service.held["cleanup.json"] != nil }
    viewer.ask(.restart)
    service.resume("cleanup.json")
    try await settle("the second preview never came back") { viewer.session.cleaningUp == nil }
    #expect(viewer.session.alert == .restart, "the alert that went up meanwhile stays")
    viewer.close()
  }

  @Test("the button stays disabled while a cleanup that got no reply may still run")
  func theButtonStaysDisabledWhileACleanupThatGotNoReplyMayStillRun() async throws {
    let service = ScriptedService()
    service.sessionList = HostSessionList(sessions: [Self.record(copies: [HostSessionCopy(app: "Probe")])])
    service.cleanupPreview = Self.preview(active: false)
    let viewer = try await ViewerController.watching(service)
    viewer.send(.chooseSession(.one(Self.key)))
    try await settle("the list was never read") { viewer.panes.sessions.list != nil }
    let cleanup = { ViewerPresenter.details(viewer.snapshot).hostSession?.cleanup }
    viewer.send(.askCleanup(session: Self.key))
    try await settle("the alert never asked") { viewer.session.alert != nil }

    service.cleanupUnanswered = true
    service.cleanupPreview = CleanupReply(
      ok: true,
      preview: true,
      steps: [
        CleanupStep(number: 1, kind: .quit, apps: ["Probe"], state: .done),
        CleanupStep(number: 2, kind: .removeCopy, app: "Probe", state: .started),
      ]
    )
    viewer.send(.alertConfirm)
    try await settle("Clean Up never ran") { service.cleanups.contains("apply \(Self.key)") }
    try await settle("the list was never read again") { service.cleanups.count >= 3 }
    await afterQueuedWork()
    #expect(viewer.session.alert == nil && !viewer.session.alertRunning)
    #expect(viewer.session.notice == "The cleanup is taking a while. Details shows each step once it ends.")
    #expect(cleanup()?.isEnabled == false, "a step is still under way, so the cleanup may still run")
    viewer.send(.askCleanup(session: Self.key))
    await afterQueuedWork()
    #expect(service.cleanups.filter { $0.hasPrefix("apply") }.count == 1, "nothing is sent while it may run")

    service.cleanupPreview = CleanupReply(
      ok: true,
      preview: true,
      steps: [
        CleanupStep(number: 1, kind: .quit, apps: ["Probe"], state: .done),
        CleanupStep(number: 2, kind: .removeCopy, app: "Probe", state: .failed, reason: "in-use", retry: true),
      ]
    )
    let checks = service.cleanups.count
    viewer.panesModel.followSessions(readNow: true)
    try await settle("the cleanup was never read again") { cleanup()?.isEnabled == true }
    #expect(service.cleanups.count > checks)
    #expect(viewer.session.cleanupUnanswered == nil)
    let failed = Fact(name: "Remove the copy of Probe", value: "Couldn't: still in use. Try again.")
    #expect(cleanup()?.receipt.last == failed, "what each step did stays for Details")
    #expect(viewer.session.notice == "The cleanup didn't finish. Details lists each step.")
    viewer.close()
  }
}
