import AsyncSupportTesting
import AtlasServiceTesting
import AtlasStatus
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("ViewerControl")
struct ViewerControlTests {
  @Test("taking control holds a lease, and giving it back releases it")
  func takingControlHoldsALeaseAndGivingItBackReleasesIt() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.control.takeControl()
    #expect(viewer.control.lease == "lease")
    #expect(service.log.contains { $0.hasPrefix("acquire frame-") })
    viewer.control.releaseControl()
    #expect(viewer.control.lease == nil)
    try await settle("the lease was never released") { service.log.contains("release lease") }
    viewer.close()
  }

  @Test("after Give Back the next reading starts at once, not after the pause")
  func afterGiveBackTheNextReadingComesAtOnce() async throws {
    let service = ScriptedService()
    // The pause between readings never ends by itself: a reading that waits for it never comes.
    let viewer = try await ViewerController.watching(service)
    await viewer.control.takeControl()
    service.hold = ["status"]
    try await settle("no reading started while in control") { service.held["status"] != nil }
    let asked = service.statusCalls
    viewer.control.releaseControl()
    try await settle("the release never read the status") { service.answered == asked }
    service.resume("status")
    try await settle("the next reading waited for the pause") { service.statusCalls > asked + 1 }
    viewer.close()
  }

  @Test("leaving the window gives control back")
  func leavingTheWindowGivesControlBack() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.control.takeControl()
    viewer.control.lostFocus()
    #expect(viewer.control.lease == nil)
    try await settle("the lease was never released") { service.log.contains("release lease") }
    viewer.close()
  }

  @Test("a grant that arrives after leaving the window is given back")
  func aGrantThatArrivesAfterLeavingIsGivenBack() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    viewer.control.gainedFocus()
    service.hold = ["acquire"]
    let taking = Task { await viewer.control.takeControl() }
    try await settle("control was never asked for") { service.held["acquire"] != nil }
    #expect(viewer.control.acquiring)
    viewer.control.lostFocus()
    service.resume("acquire")
    await taking.value
    #expect(viewer.control.lease == nil)
    #expect(!viewer.control.acquiring)
    try await settle("the late grant was never given back") { service.log.contains("release lease") }
    viewer.close()
  }

  @Test("a grant that arrives while hidden or paused is given back", arguments: [true, false])
  func aGrantThatArrivesWhileHiddenOrPausedIsGivenBack(hidden: Bool) async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    service.hold = ["acquire"]
    let taking = Task { await viewer.control.takeControl() }
    try await settle("control was never asked for") { service.held["acquire"] != nil }
    #expect(viewer.control.acquiring)
    if hidden {
      viewer.setVisible(false)
    } else {
      viewer.togglePause()
    }
    service.resume("acquire")
    await taking.value
    #expect(viewer.control.lease == nil)
    #expect(!viewer.control.acquiring)
    try await settle("the late grant was never given back") { service.log.contains("release lease") }
    viewer.close()
  }

  @Test("control can be taken again after coming back to the window, once its release is answered")
  func controlCanBeTakenAgainAfterComingBackToTheWindow() async throws {
    let viewer = try await ViewerController.watching(ScriptedService())
    await viewer.control.takeControl()
    viewer.control.lostFocus()
    viewer.control.gainedFocus()
    // Take Control waits until the release is answered.
    try await settle("the release was never answered") { !viewer.barrier.isBusy }
    await viewer.control.takeControl()
    #expect(viewer.control.lease == "lease")
    viewer.close()
  }

  @Test("a status that is not ready ends control at once, saying why in the status's own words")
  func aStatusThatIsNotReadyEndsControlAtOnce() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.control.takeControl()
    service.reply = [
      "ready": false, "canObserve": true,
      "overview": ["mode": "loggedOut", "reason": "The helper is logged out, so agents can't use it"],
    ]
    await viewer.readingLoop.readStatus()
    #expect(viewer.control.lease == nil)
    #expect(viewer.session.notice == "Control ended. The helper is logged out, so agents can't use it.")
    viewer.close()
  }

  @Test("a status that says no viewer holds control any more, as after Release Control in the menu bar, ends it")
  func aStatusWithNoViewerInControlEndsIt() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.control.takeControl()
    try await service.revoke()
    await viewer.readingLoop.readStatus()
    try await settle("control never ended") { viewer.control.lease == nil }
    #expect(viewer.session.notice == ViewerWords.releasedElsewhere)
    viewer.close()
  }

  @Test("a status asked before the grant never ends the control it could not know of")
  func aStatusAskedBeforeTheGrantKeepsControl() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    service.hold = ["status"]
    viewer.readingLoop.refresh()
    try await settle("no reading started") { service.held["status"] != nil }
    let asked = service.statusCalls
    await viewer.control.takeControl()
    service.resume("status")
    // The reading under way, which says no viewer holds control, then the one the grant asked for.
    try await settle("the reading after the grant never ended") { service.answered > asked }
    #expect(viewer.control.lease == "lease")
    viewer.close()
  }

  @Test("hiding the window gives control back")
  func hidingTheWindowGivesControlBack() async throws {
    let viewer = try await ViewerController.watching(ScriptedService())
    await viewer.control.takeControl()
    viewer.setVisible(false)
    #expect(viewer.control.lease == nil)
    viewer.close()
  }

  @Test("a stale picture ends control without further input")
  func aStalePictureEndsControlWithoutFurtherInput() async throws {
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(ScriptedService(), clock: clock)
    await viewer.control.takeControl()
    viewer.readingLoop.watch(false)
    try await clock.untilSleeping(for: ViewerControl.heartbeatEvery, "the heartbeat never slept")
    // No picture arrives as the time passes; the heartbeat that wakes past it ends control.
    clock.advance(by: .seconds(Freshness.stale))
    #expect(!viewer.control.canInput)
    try await settle("the stale picture never ended control") { viewer.control.lease == nil }
    #expect(viewer.session.notice == ViewerWords.stalePicture)
    viewer.close()
  }

  @Test("a failed renewal ends control")
  func aFailedRenewalEndsControl() async throws {
    let service = ScriptedService()
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(service, clock: clock)
    service.fail = ["renew"]
    await viewer.control.takeControl()
    // Readings stop, so the heartbeat is the clock's one sleeper; the picture stays fresh through five ticks.
    viewer.readingLoop.watch(false)
    for tick in 1...ViewerControl.renewEvery {
      try await clock.untilSleeping(for: ViewerControl.heartbeatEvery, "heartbeat \(tick) never slept")
      clock.advance(by: ViewerControl.heartbeatEvery)
    }
    try await settle("the failed renewal never ended control") { viewer.control.lease == nil }
    #expect(service.log.contains("renew"))
    #expect(viewer.session.notice == "renew failed")
    viewer.close()
  }

  @Test("an input that fails ends control")
  func anInputThatFailsEndsControl() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    service.fail = ["input"]
    await viewer.control.takeControl()
    viewer.control.sendKey("escape")
    try await settle("the failed input never ended control") { viewer.control.lease == nil }
    #expect(viewer.session.notice == "input failed")
    viewer.close()
  }

  @Test("a changed helper ends control")
  func aChangedHelperEndsControl() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.control.takeControl()
    service.revision = "r2"
    viewer.readingLoop.refresh()
    try await settle("the changed picture never ended control") { viewer.control.lease == nil }
    #expect(viewer.session.notice == ViewerWords.helperChanged)
    viewer.close()
  }

  @Test("a changed helper says nothing to a viewer that only watches, and its notice, such as Recover's, stays")
  func aChangedHelperLeavesWatchingQuiet() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    viewer.say(Words.recovered)
    service.revision = "r2"
    viewer.readingLoop.refresh()
    try await settle("the changed picture never showed") { viewer.reading.frame?.revision == "r2" }
    #expect(viewer.session.notice == Words.recovered, "no control was held, so none ended")
    viewer.close()
  }

  @Test("Take Control pressed while an agent acts retries once and then says the agent is mid-action")
  func takeControlRetriesOnceWhileAnAgentActs() async throws {
    let service = ScriptedService()
    service.reply = ["ready": true, "canObserve": true, "overview": ["mode": "agent", "state": "working"]]
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(service, clock: clock)
    // Readings stop, so the retry is the clock's one sleeper.
    viewer.readingLoop.watch(false)
    #expect(viewer.state.top.actions.control.isEnabled, "Take Control stays offered while an agent works")
    service.busyAcquires = 2
    try await Self.takeControl(viewer, retryingOn: clock)
    #expect(service.log.filter { $0.hasPrefix("acquire") }.count == 2, "asked once, then once more")
    #expect(viewer.control.lease == nil)
    #expect(viewer.session.notice == ViewerWords.agentMidAction)
    service.busyAcquires = 1
    try await Self.takeControl(viewer, retryingOn: clock)
    #expect(viewer.control.lease == "lease", "the action ended within the wait, so the retry took control")
    viewer.close()
  }

  @Test("a held gesture ends when its clock passes the hold")
  func aHeldGestureEndsWhenItsClockPassesTheHold() async throws {
    let service = ScriptedService()
    service.busyAcquires = 1
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(service, clock: clock)
    // Readings stop, so the retry is the clock's one sleeper.
    viewer.readingLoop.watch(false)
    // Take Control, answered busy, is held for the retry.
    let taking = Task { await viewer.control.takeControl() }
    try await clock.untilSleeping(for: ViewerControl.busyRetryAfter, "Take Control was never held")
    #expect(clock.waiting == [ViewerControl.busyRetryAfter])
    clock.advance(by: ViewerControl.busyRetryAfter - .milliseconds(1))
    #expect(clock.waiting == [.milliseconds(1)], "the hold lasts its whole second")
    #expect(viewer.control.acquiring)
    clock.advance(by: .milliseconds(1))
    try await taking.value("the held Take Control never ended")
    #expect(viewer.control.lease == "lease")
    #expect(!viewer.control.acquiring)
    viewer.close()
  }

  /// Takes control on a viewer that has stopped reading, moving the clock past the busy retry's wait.
  ///
  /// - Parameters:
  ///   - viewer: The viewer.
  ///   - clock: Its clock.
  /// - Throws: The failed requirement, when Take Control never waits for its retry or never ends.
  private static func takeControl(_ viewer: ViewerController, retryingOn clock: ManualClock) async throws {
    let taking = Task { await viewer.control.takeControl() }
    try await clock.untilSleeping(for: ViewerControl.busyRetryAfter, "Take Control never waited to ask again")
    clock.advance(by: ViewerControl.busyRetryAfter)
    try await taking.value("Take Control never ended")
  }

  @Test("text being composed is dropped once the picture is lost mid-composition")
  func losingThePictureDropsTheComposition() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.control.takeControl()
    viewer.control.compose("ni")
    service.fail = ["status"]
    viewer.readingLoop.refresh()
    try await settle("the failed reading never showed") { viewer.reading.unavailable != nil }
    #expect(viewer.control.composing.isEmpty)
    viewer.close()
  }

  @Test("the viewer's control owns its generation, so a reply from an older lease is dropped")
  func anOlderLeasesReplyIsDropped() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.control.takeControl()
    try #require(viewer.control.lease == "lease")
    service.fail = ["release"]
    service.hold = ["release"]
    viewer.control.releaseControl()
    let released = viewer.control.controlGeneration
    try await settle("the release was never sent") { service.held["release"] != nil }
    // Something newer happens before the service answers: leaving the window moves the generation on.
    viewer.control.lostFocus()
    #expect(viewer.control.controlGeneration > released)
    service.resume("release")
    try await settle("the refused release never came back") {
      !viewer.barrier.isBusy && viewer.barrier.unreleased == ["lease"]
    }
    #expect(viewer.session.notice == nil, "the older lease's refusal says nothing")
    #expect(viewer.control.lease == nil)
    viewer.close()
  }
}
