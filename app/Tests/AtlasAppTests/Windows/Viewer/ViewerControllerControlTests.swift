import AtlasKit
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@MainActor
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
    let viewer = try await ViewerController.watching(service, nap: ViewerController.untilCancelled)
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

  @Test("control can be taken again after coming back to the window")
  func controlCanBeTakenAgainAfterComingBackToTheWindow() async throws {
    let viewer = try await ViewerController.watching(ScriptedService())
    await viewer.takeControl()
    viewer.lostFocus()
    viewer.gainedFocus()
    await viewer.takeControl()
    #expect(viewer.session.lease == "lease")
    viewer.close()
  }

  @Test("a status that is not ready ends control at once")
  func aStatusThatIsNotReadyEndsControlAtOnce() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.takeControl()
    service.reply = ["ready": false, "canObserve": true, "overview": ["mode": "observing"]]
    await viewer.readStatus()
    #expect(viewer.session.lease == nil)
    #expect(viewer.session.notice == ViewerWords.inputUnavailable)
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
    let clock = TestClock()
    let viewer = try await ViewerController.watching(ScriptedService(), clock: clock.read, heartbeat: .milliseconds(5))
    await viewer.takeControl()
    viewer.watch(false)
    clock.now = Freshness.stale
    #expect(!viewer.canInput)
    try await settle("the stale picture never ended control") { viewer.session.lease == nil }
    #expect(viewer.session.notice == ViewerWords.stalePicture)
    viewer.close()
  }

  @Test("a failed renewal ends control")
  func aFailedRenewalEndsControl() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service, heartbeat: .milliseconds(5))
    service.fail = ["renew"]
    await viewer.takeControl()
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

  @Test("a changed helper says nothing to a viewer that only watches")
  func aChangedHelperLeavesWatchingQuiet() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    service.revision = "r2"
    viewer.refresh()
    try await settle("the changed picture never showed") { viewer.reading.frame?.revision == "r2" }
    #expect(viewer.session.notice == nil, "no control was held, so none ended")
    viewer.close()
  }

  @Test("control waits while an agent works")
  func controlWaitsWhileAnAgentWorks() async throws {
    let service = ScriptedService()
    service.reply = ["ready": true, "canObserve": true, "overview": ["mode": "agent"]]
    let viewer = try await ViewerController.watching(service)
    #expect(!viewer.snapshot.canTakeControl)
    await viewer.takeControl()
    #expect(!service.log.contains { $0.hasPrefix("acquire") }, "nothing is asked while control must wait")
    viewer.close()
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
