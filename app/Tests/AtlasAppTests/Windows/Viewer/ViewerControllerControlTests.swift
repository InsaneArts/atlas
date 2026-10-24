import AsyncSupportTesting
import AtlasServiceTesting
import AtlasStatus
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("ViewerController+Control")
struct ViewerControllerControlTests {
  @Test("taking control holds a lease, and giving it back releases it")
  func takingControlHoldsALeaseAndGivingItBackReleasesIt() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.takeControl()
    #expect(viewer.session.lease == "lease")
    #expect(service.log.contains { $0.hasPrefix("acquire frame-") })
    viewer.releaseControl()
    #expect(viewer.session.lease == nil)
    try await settle("the lease was never released") { service.log.contains("release lease") }
    viewer.close()
  }

  @Test("after Give Back the next reading starts at once, not after the pause")
  func afterGiveBackTheNextReadingComesAtOnce() async throws {
    let service = ScriptedService()
    // The pause between readings never ends by itself: a reading that waits for it never comes.
    let viewer = try await ViewerController.watching(service)
    await viewer.takeControl()
    service.hold = ["status"]
    try await settle("no reading started while in control") { service.held["status"] != nil }
    let asked = service.statusCalls
    viewer.releaseControl()
    try await settle("the release never read the status") { service.answered == asked }
    service.resume("status")
    try await settle("the next reading waited for the pause") { service.statusCalls > asked + 1 }
    viewer.close()
  }

  @Test("leaving the window gives control back")
  func leavingTheWindowGivesControlBack() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.takeControl()
    viewer.lostFocus()
    #expect(viewer.session.lease == nil)
    try await settle("the lease was never released") { service.log.contains("release lease") }
    viewer.close()
  }

  @Test("a grant that arrives after leaving the window is given back")
  func aGrantThatArrivesAfterLeavingIsGivenBack() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    viewer.gainedFocus()
    service.hold = ["acquire"]
    let taking = Task { await viewer.takeControl() }
    try await settle("control was never asked for") { service.held["acquire"] != nil }
    #expect(viewer.session.acquiring)
    viewer.lostFocus()
    service.resume("acquire")
    await taking.value
    #expect(viewer.session.lease == nil)
    #expect(!viewer.session.acquiring)
    try await settle("the late grant was never given back") { service.log.contains("release lease") }
    viewer.close()
  }

  @Test("a grant that arrives while hidden or paused is given back", arguments: [true, false])
  func aGrantThatArrivesWhileHiddenOrPausedIsGivenBack(hidden: Bool) async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    service.hold = ["acquire"]
    let taking = Task { await viewer.takeControl() }
    try await settle("control was never asked for") { service.held["acquire"] != nil }
    #expect(viewer.session.acquiring)
    if hidden {
      viewer.setVisible(false)
    } else {
      viewer.togglePause()
    }
    service.resume("acquire")
    await taking.value
    #expect(viewer.session.lease == nil)
    #expect(!viewer.session.acquiring)
    try await settle("the late grant was never given back") { service.log.contains("release lease") }
    viewer.close()
  }

  @Test("control can be taken again after coming back to the window, once its release is answered")
  func controlCanBeTakenAgainAfterComingBackToTheWindow() async throws {
    let viewer = try await ViewerController.watching(ScriptedService())
    await viewer.takeControl()
    viewer.lostFocus()
    viewer.gainedFocus()
    // Take Control waits until the release is answered.
    try await settle("the release was never answered") { !viewer.barrier.isBusy }
    await viewer.takeControl()
    #expect(viewer.session.lease == "lease")
    viewer.close()
  }

  @Test("a status that is not ready ends control at once, saying why in the status's own words")
  func aStatusThatIsNotReadyEndsControlAtOnce() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.takeControl()
    service.reply = [
      "ready": false, "canObserve": true,
      "overview": ["mode": "loggedOut", "reason": "The helper is logged out, so agents can't use it"],
    ]
    await viewer.readStatus()
    #expect(viewer.session.lease == nil)
    #expect(viewer.session.notice == "Control ended. The helper is logged out, so agents can't use it.")
    viewer.close()
  }

  @Test("a status that says no viewer holds control any more, as after Release Control in the menu bar, ends it")
  func aStatusWithNoViewerInControlEndsIt() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.takeControl()
    try await service.revoke()
    await viewer.readStatus()
    try await settle("control never ended") { viewer.session.lease == nil }
    #expect(viewer.session.notice == ViewerWords.releasedElsewhere)
    viewer.close()
  }

  @Test("a status asked before the grant never ends the control it could not know of")
  func aStatusAskedBeforeTheGrantKeepsControl() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    service.hold = ["status"]
    viewer.refresh()
    try await settle("no reading started") { service.held["status"] != nil }
    let asked = service.statusCalls
    await viewer.takeControl()
    service.resume("status")
    // The reading under way, which says no viewer holds control, then the one the grant asked for.
    try await settle("the reading after the grant never ended") { service.answered > asked }
    #expect(viewer.session.lease == "lease")
    viewer.close()
  }

  @Test("hiding the window gives control back")
  func hidingTheWindowGivesControlBack() async throws {
    let viewer = try await ViewerController.watching(ScriptedService())
    await viewer.takeControl()
    viewer.setVisible(false)
    #expect(viewer.session.lease == nil)
    viewer.close()
  }

  @Test("a stale picture ends control without further input")
  func aStalePictureEndsControlWithoutFurtherInput() async throws {
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(ScriptedService(), clock: clock)
    await viewer.takeControl()
    viewer.watch(false)
    try await clock.untilSleeping(for: ViewerController.heartbeatEvery, "the heartbeat never slept")
    // No picture arrives as the time passes; the heartbeat that wakes past it ends control.
    clock.advance(by: .seconds(Freshness.stale))
    #expect(!viewer.canInput)
    try await settle("the stale picture never ended control") { viewer.session.lease == nil }
    #expect(viewer.session.notice == ViewerWords.stalePicture)
    viewer.close()
  }

  @Test("a failed renewal ends control")
  func aFailedRenewalEndsControl() async throws {
    let service = ScriptedService()
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(service, clock: clock)
    service.fail = ["renew"]
    await viewer.takeControl()
    // Readings stop, so the heartbeat is the clock's one sleeper; the picture stays fresh through five ticks.
    viewer.watch(false)
    for tick in 1...ViewerController.renewEvery {
      try await clock.untilSleeping(for: ViewerController.heartbeatEvery, "heartbeat \(tick) never slept")
      clock.advance(by: ViewerController.heartbeatEvery)
    }
    try await settle("the failed renewal never ended control") { viewer.session.lease == nil }
    #expect(service.log.contains("renew"))
    #expect(viewer.session.notice == "renew failed")
    viewer.close()
  }

  @Test("an input that fails ends control")
  func anInputThatFailsEndsControl() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    service.fail = ["input"]
    await viewer.takeControl()
    viewer.sendKey("escape")
    try await settle("the failed input never ended control") { viewer.session.lease == nil }
    #expect(viewer.session.notice == "input failed")
    viewer.close()
  }

  @Test("a changed helper ends control")
  func aChangedHelperEndsControl() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.takeControl()
    service.revision = "r2"
    viewer.refresh()
    try await settle("the changed picture never ended control") { viewer.session.lease == nil }
    #expect(viewer.session.notice == ViewerWords.helperChanged)
    viewer.close()
  }

  @Test("a changed helper says nothing to a viewer that only watches, and its notice, such as Recover's, stays")
  func aChangedHelperLeavesWatchingQuiet() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    viewer.say(Words.recovered)
    service.revision = "r2"
    viewer.refresh()
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
    viewer.watch(false)
    #expect(viewer.state.top.actions.control.isEnabled, "Take Control stays offered while an agent works")
    service.busyAcquires = 2
    try await Self.takeControl(viewer, retryingOn: clock)
    #expect(service.log.filter { $0.hasPrefix("acquire") }.count == 2, "asked once, then once more")
    #expect(viewer.session.lease == nil)
    #expect(viewer.session.notice == ViewerWords.agentMidAction)
    service.busyAcquires = 1
    try await Self.takeControl(viewer, retryingOn: clock)
    #expect(viewer.session.lease == "lease", "the action ended within the wait, so the retry took control")
    viewer.close()
  }

  @Test("a held gesture ends when its clock passes the hold")
  func aHeldGestureEndsWhenItsClockPassesTheHold() async throws {
    let service = ScriptedService()
    service.busyAcquires = 1
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(service, clock: clock)
    // Readings stop, so the retry is the clock's one sleeper.
    viewer.watch(false)
    // Take Control, answered busy, is held for the retry.
    let taking = Task { await viewer.takeControl() }
    try await clock.untilSleeping(for: ViewerController.busyRetryAfter, "Take Control was never held")
    #expect(clock.waiting == [ViewerController.busyRetryAfter])
    clock.advance(by: ViewerController.busyRetryAfter - .milliseconds(1))
    #expect(clock.waiting == [.milliseconds(1)], "the hold lasts its whole second")
    #expect(viewer.session.acquiring)
    clock.advance(by: .milliseconds(1))
    try await taking.value("the held Take Control never ended")
    #expect(viewer.session.lease == "lease")
    #expect(!viewer.session.acquiring)
    viewer.close()
  }

  /// Takes control on a viewer that has stopped reading, moving the clock past the busy retry's wait.
  ///
  /// - Parameters:
  ///   - viewer: The viewer.
  ///   - clock: Its clock.
  /// - Throws: The failed requirement, when Take Control never waits for its retry or never ends.
  private static func takeControl(_ viewer: ViewerController, retryingOn clock: ManualClock) async throws {
    let taking = Task { await viewer.takeControl() }
    try await clock.untilSleeping(for: ViewerController.busyRetryAfter, "Take Control never waited to ask again")
    clock.advance(by: ViewerController.busyRetryAfter)
    try await taking.value("Take Control never ended")
  }

  @Test("text being composed is dropped once the picture is lost mid-composition")
  func losingThePictureDropsTheComposition() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.takeControl()
    viewer.compose("ni")
    service.fail = ["status"]
    viewer.refresh()
    try await settle("the failed reading never showed") { viewer.reading.unavailable != nil }
    #expect(viewer.session.composing.isEmpty)
    viewer.close()
  }
}
