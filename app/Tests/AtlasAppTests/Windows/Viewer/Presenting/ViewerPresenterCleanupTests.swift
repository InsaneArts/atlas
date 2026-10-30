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
    snapshot.session.chosenSession = .one(record.key)
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

  /// A preview of quitting Probe and Notes, rolling back a plan, moving Probe aside in the helper and removing two copies.
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

  @Test("Clean Up is offered only when the session left something")
  func cleanUpIsOfferedOnlyWhenTheSessionLeftSomething() throws {
    let cleanup = { (record: HostSessionRecord) in
      ViewerPresenter.chosenCleanup(try Self.chosen(record))
    }
    #expect(try cleanup(Self.record()) == nil, "a session that left nothing offers none")
    #expect(try cleanup(Self.record(cleanup: .complete)) == nil, "a cleanup that finished leaves nothing")
    let copied = try #require(try cleanup(Self.record(copies: [HostSessionCopy(app: "Opie")])))
    #expect(copied.session == Self.key && copied.stage == .ready && copied.isEnabled)
    #expect(try cleanup(Self.record(installs: ["Probe"])) != nil, "an app its task installed is left")
    #expect(try cleanup(Self.record(cleanup: .incomplete)) != nil, "a cleanup that did not finish is left")
    let terminal = Self.record(key: "", copies: [HostSessionCopy(app: "Opie")])
    #expect(try cleanup(terminal) == nil, "Terminal's session has no key a cleanup can name")
  }

  @Test("the confirmation lists what will be quit and moved aside")
  func theConfirmationListsWhatWillBeQuitAndMovedAside() throws {
    let steps = ViewerPresenter.cleanupSteps(Self.preview(active: false))
    #expect(
      steps == [
        "Quits Probe and Notes",
        "Undoes 1 environment change",
        "Moves Probe aside in the helper; removed after 7 days",
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
      steps: [CleanupStep(number: 1, kind: .removeCopy, state: .blocked, reason: .stopped)]
    )
    #expect(
      ViewerPresenter.cleanupNotice(blocked)
        == "Agents are stopped, so the cleanup stopped. Resume, then clean up again."
    )
    let failed = CleanupReply(state: .incomplete, steps: [CleanupStep(number: 1, kind: .removeCopy, state: .failed)])
    #expect(
      ViewerPresenter.cleanupNotice(failed) == "The cleanup didn't finish. The session's tooltip lists each step."
    )
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
        CleanupStep(number: 2, kind: .removeInstall, app: "Probe", state: .kept, reason: .changed),
        CleanupStep(number: 3, kind: .removeCopy, app: "Probe", state: .failed, reason: .inUse, retry: true),
        CleanupStep(number: 4, kind: .removeCopy, state: .failed, reason: .io),
        CleanupStep(number: 5, kind: .removeCopy, app: "Notes", state: .done, alreadyGone: true),
        CleanupStep(number: 6, kind: .undoPlan, state: .uncertain),
        CleanupStep(number: 7, kind: .unknown, state: .pending),
        CleanupStep(number: 8, kind: .removeCopy, app: "Mail", state: .kept, reason: .otherVolume),
        CleanupStep(number: 9, kind: .removeInstall, app: "Mail", state: .kept, reason: .unreadable),
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
        Fact(name: "Remove the copy of Mail", value: "Kept: it could not be moved aside without copying"),
        Fact(name: "Remove Mail from Applications", value: "Kept: Atlas couldn't read its record of it"),
      ]
    )
    #expect(
      ViewerPresenter.cleanupNotice(applied) == "The cleanup didn't finish. The session's tooltip lists each step."
    )
    #expect(ViewerPresenter.cleanupNotice(CleanupReply(ok: true, state: .complete)) == ViewerWords.nothingToCleanUp)
    let unknown = CleanupStep(number: 1, kind: .removeCopy, state: .kept, reason: .unknown)
    #expect(ViewerPresenter.cleanupOutcome(unknown) == "Kept", "a reason this app does not know is never shown")
    var snapshot = try Self.chosen(Self.record(copies: [HostSessionCopy(app: "Probe")]))
    snapshot.session.cleanup.receipts[Self.key] = applied
    let cleanup = try #require(ViewerPresenter.chosenCleanup(snapshot))
    #expect(cleanup.receipt.count == 9, "the receipt is kept, and Clean Up can be chosen again")
    #expect(cleanup.isEnabled)
    let row = try ViewerPresenterSessionMenuTests.row(Self.key, snapshot)
    #expect(row.help.contains("Last cleanup\nQuit Probe: Done"), "the row's tooltip lists the receipt")
  }

  @Test("an install moved aside in the helper says so in the receipt and a copy reads as done")
  func anInstallMovedAsideInTheHelperSaysSoInTheReceiptAndACopyReadsAsDone() {
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
          held: "/Users/atlas/.config/atlas/installs/held/a7d2c0ffee12/Probe.app"
        ),
        CleanupStep(number: 4, kind: .removeCopy, app: "TextEdit", state: .done),
        CleanupStep(number: 5, kind: .removeCopy, app: "Notes", state: .done, alreadyGone: true),
      ]
    )
    #expect(
      ViewerPresenter.cleanupReceipt(applied) == [
        Fact(name: "Quit Probe", value: "Done"),
        Fact(name: "Undo an environment change", value: "Done"),
        Fact(name: "Remove Probe from Applications", value: "Moved aside in the helper; removed after 7 days"),
        Fact(name: "Remove the copy of TextEdit", value: "Done"),
        Fact(name: "Remove the copy of Notes", value: "Done: it was already gone"),
      ]
    )
    #expect(ViewerPresenter.cleanupNotice(applied) == "Cleaned up this session's work.")
  }

  @Test("Clean Up shows its progress on the row and its receipt in the footer")
  func cleanUpShowsItsProgressOnTheRowAndItsReceiptInTheFooter() async throws {
    let service = ScriptedService()
    // The status lists the session live, so the sidebar draws its row.
    service.reply["overview"] = ["mode": "observing", "hostSessions": [SessionReplies.claudeActing]]
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
    let cleanup = { ViewerPresenter.chosenCleanup(viewer.snapshot) }
    let row = { try ViewerPresenterSessionMenuTests.row(Self.key, viewer.snapshot) }
    #expect(cleanup()?.isEnabled == true)
    #expect(try !row().isCleaningUp)

    service.hold = ["cleanup.json"]
    viewer.send(.menu(.askCleanup(session: Self.key)))
    try await settle("the preview was never asked for") { service.held["cleanup.json"] != nil }
    #expect(cleanup()?.stage == .working, "it waits while what it takes is read")
    #expect(try row().isCleaningUp && row().line == "Cleaning up…" && row().badge == nil, "the row says so")
    viewer.send(.menu(.askCleanup(session: Self.key)))
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
    #expect(try row().isCleaningUp, "the row shows it working")
    #expect(viewer.state.alert?.isBusy == true, "the alert's buttons wait too")
    viewer.send(.alertConfirm)
    await afterQueuedWork()
    service.resume("cleanup")
    try await settle("the cleanup never ended") { !viewer.session.alertRunning && viewer.session.alert == nil }
    #expect(service.cleanups == ["preview \(Self.key)", "apply \(Self.key)"], "Clean Up runs once")
    #expect(viewer.session.notice == "Cleaned up this session's work.")
    #expect(viewer.state.footer.latest.words == "Cleaned up this session's work.", "the footer gives the notice")
    #expect(cleanup()?.receipt == [Fact(name: "Remove the copy of Probe", value: "Done")])
    #expect(cleanup()?.isEnabled == true)
    #expect(try !row().isCleaningUp)
    #expect(try row().help.hasSuffix("Last cleanup\nRemove the copy of Probe: Done"), "its tooltip lists the receipt")
    viewer.close()
  }

  @Test("another session's cleanup holds Clean Up without a spinner of its own")
  func anotherSessionsCleanupHoldsCleanUpWithoutASpinner() throws {
    var snapshot = try Self.chosen(Self.record(copies: [HostSessionCopy(app: "Probe")]))
    snapshot.session.cleanup.running = "8be20c4d1a77"
    let held = try #require(ViewerPresenter.chosenCleanup(snapshot))
    #expect(!held.isEnabled && !held.isWorking && held.waitsForAnother)
    snapshot.session.cleanup.running = nil
    snapshot.session.cleanup.unanswered = ViewerUnansweredCleanup(session: "8be20c4d1a77", sentAt: .distantPast)
    let waiting = try #require(ViewerPresenter.chosenCleanup(snapshot))
    #expect(!waiting.isEnabled && !waiting.isWorking)
    snapshot.session.cleanup.unanswered = ViewerUnansweredCleanup(session: Self.key, sentAt: .distantPast)
    let own = try #require(ViewerPresenter.chosenCleanup(snapshot))
    #expect(!own.isEnabled && own.isWorking && !own.waitsForAnother)
    #expect(try ViewerPresenterSessionMenuTests.row(Self.key, snapshot).line == "Cleaning up…", "its row says so")
  }

  @Test("what the last cleanups did is forgotten for sessions the list no longer names")
  func receiptsAreForgottenForSessionsTheListNoLongerNames() {
    var state = ViewerCleanupState()
    state.receipts = [Self.key: CleanupReply(ok: true), "c0ffee5e5510": CleanupReply(ok: true)]
    state.keepReceipts(for: [Self.key])
    #expect(Array(state.receipts.keys) == [Self.key])
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
    viewer.send(.menu(.askCleanup(session: Self.key)))
    try await settle("the preview was never asked for") { service.held["cleanup.json"] != nil }
    viewer.send(.chooseSession(.all))
    service.resume("cleanup.json")
    try await settle("the preview never came back") { viewer.session.cleanup.running == nil }
    #expect(viewer.session.alert == nil, "another session is shown, so nothing asks about this one")

    viewer.send(.chooseSession(.one(Self.key)))
    service.hold = ["cleanup.json"]
    viewer.send(.menu(.askCleanup(session: Self.key)))
    try await settle("the second preview was never asked for") { service.held["cleanup.json"] != nil }
    viewer.ask(.restart)
    service.resume("cleanup.json")
    try await settle("the second preview never came back") { viewer.session.cleanup.running == nil }
    #expect(viewer.session.alert == .restart, "the alert that went up meanwhile stays")
    viewer.close()
  }

  @Test("Clean Up waits while a cleanup that got no reply may still run")
  func cleanUpWaitsWhileACleanupThatGotNoReplyMayStillRun() async throws {
    let service = ScriptedService()
    service.sessionList = HostSessionList(sessions: [Self.record(copies: [HostSessionCopy(app: "Probe")])])
    service.cleanupPreview = Self.preview(active: false)
    let viewer = try await ViewerController.watching(service)
    viewer.send(.chooseSession(.one(Self.key)))
    try await settle("the list was never read") { viewer.panes.sessions.list != nil }
    let cleanup = { ViewerPresenter.chosenCleanup(viewer.snapshot) }
    viewer.send(.menu(.askCleanup(session: Self.key)))
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
    #expect(
      viewer.session.notice == "The cleanup is taking a while. The session's tooltip lists each step once it ends."
    )
    #expect(cleanup()?.isEnabled == false, "a step is still under way, so the cleanup may still run")
    viewer.send(.menu(.askCleanup(session: Self.key)))
    await afterQueuedWork()
    #expect(service.cleanups.filter { $0.hasPrefix("apply") }.count == 1, "nothing is sent while it may run")

    service.cleanupPreview = CleanupReply(
      ok: true,
      preview: true,
      steps: [
        CleanupStep(number: 1, kind: .quit, apps: ["Probe"], state: .done),
        CleanupStep(number: 2, kind: .removeCopy, app: "Probe", state: .failed, reason: .inUse, retry: true),
      ]
    )
    let checks = service.cleanups.count
    viewer.panesModel.followSessions(readNow: true)
    try await settle("the cleanup was never read again") { cleanup()?.isEnabled == true }
    #expect(service.cleanups.count > checks)
    #expect(viewer.session.cleanup.unanswered == nil)
    let failed = Fact(name: "Remove the copy of Probe", value: "Couldn't: still in use. Try again.")
    #expect(cleanup()?.receipt.last == failed, "what each step did stays for the session's tooltip")
    #expect(viewer.session.notice == "The cleanup didn't finish. The session's tooltip lists each step.")
    viewer.close()
  }

  @Test("the confirmation checks collected files first and names the background job it stops")
  func theConfirmationChecksCollectedFilesAndNamesTheJob() {
    let preview = CleanupReply(
      preview: true,
      steps: [
        CleanupStep(number: 1, kind: .checkOutputs),
        CleanupStep(number: 2, kind: .stopJob, job: "7c2e91a04b3f", name: "web"),
        CleanupStep(number: 3, kind: .undoPlan),
      ]
    )
    #expect(
      ViewerPresenter.cleanupSteps(preview) == [
        "Checks the files it collected are still kept",
        "Stops the background job web",
        "Undoes 1 environment change",
      ]
    )
    let two = CleanupReply(
      preview: true,
      steps: [
        CleanupStep(number: 1, kind: .stopJob, name: "web"),
        CleanupStep(number: 2, kind: .stopJob, name: "worker"),
      ]
    )
    #expect(ViewerPresenter.cleanupSteps(two) == ["Stops 2 background jobs"])
  }

  @Test("the confirmation counts every background job it stops, named or not")
  func theConfirmationCountsEveryJobNamedOrNot() {
    let unnamed = CleanupReply(preview: true, steps: [CleanupStep(number: 1, kind: .stopJob, job: "7c2e91a04b3f")])
    #expect(ViewerPresenter.cleanupSteps(unnamed) == ["Stops 1 background job"])
    let mixed = CleanupReply(
      preview: true,
      steps: [
        CleanupStep(number: 1, kind: .stopJob, name: "web"),
        CleanupStep(number: 2, kind: .stopJob, job: "7c2e91a04b3f"),
      ]
    )
    #expect(ViewerPresenter.cleanupSteps(mixed) == ["Stops 2 background jobs"])
  }

  @Test("the receipt says which processes left a stopped job and how many collected files are not kept")
  func theReceiptSaysWhatLeftAJobAndWhatIsNotKept() {
    let applied = CleanupReply(
      state: .blocked,
      steps: [
        CleanupStep(
          number: 1,
          kind: .checkOutputs,
          state: .failed,
          reason: .outputNotKept,
          retry: true,
          notKept: 2
        ),
        CleanupStep(
          number: 2,
          kind: .stopJob,
          state: .done,
          name: "web",
          left: [CleanupProcess(name: "sleep")]
        ),
        CleanupStep(number: 3, kind: .stopJob, state: .failed, reason: .stillStopping, retry: true, name: "worker"),
        CleanupStep(number: 4, kind: .stopJob, state: .failed, reason: .identity),
        CleanupStep(number: 5, kind: .stopJob, state: .failed, reason: .otherAccount),
        CleanupStep(number: 6, kind: .undoPlan, state: .blocked, reason: .outputNotKept),
      ]
    )
    #expect(
      ViewerPresenter.cleanupReceipt(applied) == [
        Fact(name: "Check the collected files", value: "Couldn't: a collected copy is missing or changed. Try again."),
        Fact(name: "Not kept here", value: "2 collected files; the helper keeps them"),
        Fact(name: "Stop the background job web", value: "Done"),
        Fact(name: "Still running", value: "sleep; Atlas didn't stop it"),
        Fact(name: "Stop the background job worker", value: "Couldn't: the job is still stopping. Try again."),
        Fact(name: "Stop a background job", value: "Couldn't: the job is no longer the process Atlas started"),
        Fact(name: "Stop a background job", value: "Couldn't: part of the job runs as another account"),
        Fact(name: "Undo an environment change", value: "Didn't run: a collected copy is missing or changed"),
      ]
    )
    #expect(ViewerPresenter.cleanupNotice(applied) == ViewerWords.cleanupOutputsNotKept)
    #expect(!ViewerPresenter.cleanupReceipt(applied).contains { $0.value.contains("/Users/atlas") }, "never a path")
  }

  @Test("a cleanup held back by a helper it could not reach says so, apart from a Stop")
  func aCleanupHeldBackByAnUnreachedHelperSaysSo() {
    let unreached = CleanupReply(
      state: .blocked,
      steps: [CleanupStep(number: 1, kind: .stopJob, state: .blocked, reason: .daemon)]
    )
    #expect(ViewerPresenter.cleanupNotice(unreached) == ViewerWords.cleanupJobsUnreached)
    #expect(ViewerWords.stepBlocked(.daemon) == "Didn't run: Atlas couldn't reach the helper")
    #expect(ViewerPresenter.unansweredCleanupNotice(unreached) == ViewerWords.cleanupJobsUnreached)
  }
}
