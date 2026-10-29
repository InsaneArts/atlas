import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import Observation
import Testing
import os

@testable import AtlasApp

@Suite("ViewerReadingLoop")
struct ViewerReadingLoopTests {
  @Test("a paused picture keeps status and controls current without fetching frames", arguments: [false, true])
  func aPausedPictureKeepsStatusAndControlsCurrent(hasPicture: Bool) async throws {
    let service = ScriptedService()
    service.picture = png()
    service.reply = ["ready": true, "canObserve": true, "overview": ["mode": "observing", "state": "idle"]]
    let clock = ManualClock()
    let viewer = ViewerController.scripted(service, clock: clock)
    defer { viewer.close() }
    if hasPicture {
      viewer.setVisible(true)
      try await settle("the first picture never arrived") { viewer.reading.frame != nil }
    }
    let asked = service.statusCalls
    let frames = service.frameCalls
    viewer.togglePause()
    if !hasPicture { viewer.setVisible(true) }
    try await settle("the paused viewer never read status") { service.statusCalls > asked }
    let interval = Duration.seconds(ViewerPictureRate.live.seconds)
    try await clock.untilSleeping(for: interval, "the paused reading never waited")
    #expect(ViewerPresenter.status(viewer.snapshot).label == "Idle")
    #expect(viewer.state.footer.controls.agent == nil)
    #expect(service.frameCalls == frames)

    service.reply = ["ready": true, "canObserve": true, "overview": ["mode": "agent", "state": "working"]]
    clock.advance(by: interval)
    try await settle("the paused viewer never showed Working") {
      ViewerPresenter.status(viewer.snapshot).label == "Working"
    }
    #expect(viewer.state.footer.controls.agent == .stop)
    #expect(service.frameCalls == frames)
    try await clock.untilSleeping(for: interval, "the working reading never waited")

    service.reply = [
      "ready": false, "canObserve": true,
      "overview": ["mode": "recovering", "state": "needsYou", "reason": "Unknown outcome"],
    ]
    clock.advance(by: interval)
    try await settle("the paused viewer never showed Needs you") { viewer.snapshot.mode.kind == .needsYou }
    #expect(viewer.state.footer.controls.agent == nil)
    #expect(viewer.state.footer.controls.control?.event == .askRecover)
    #expect(viewer.state.footer.controls.control?.isEnabled == true)
    #expect(service.frameCalls == frames)
    #expect(viewer.session.paused)
  }

  @Test("a paused picture survives a status failure and shows the next status without fetching frames")
  func aPausedPictureSurvivesAStatusFailureAndShowsTheNextStatus() async throws {
    let service = ScriptedService()
    service.reply = ["ready": true, "canObserve": true, "overview": ["mode": "observing", "state": "idle"]]
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(service, clock: clock)
    defer { viewer.close() }
    let shown = try #require(viewer.reading.picture)
    let frame = try #require(viewer.reading.frame)
    let frames = service.frameCalls
    let asked = service.statusCalls
    viewer.togglePause()
    try await settle("the paused viewer never read status") { service.statusCalls > asked }
    let interval = Duration.seconds(ViewerPictureRate.live.seconds)
    try await clock.untilSleeping(for: interval, "the paused reading never waited")

    service.fail = ["status"]
    clock.advance(by: interval)
    try await clock.untilSleeping(for: interval, "the failed reading never waited")
    #expect(viewer.reading.picture == shown)
    #expect(viewer.reading.frame?.id == frame.id)
    #expect(viewer.reading.unavailable == nil)
    #expect(viewer.state.screen.picture == shown)
    #expect(viewer.state.screen.empty == nil)
    #expect(ViewerPresenter.status(viewer.snapshot).label == "Idle")
    #expect(service.frameCalls == frames)

    service.fail = []
    service.reply = ["ready": true, "canObserve": true, "overview": ["mode": "agent", "state": "working"]]
    clock.advance(by: interval)
    try await clock.untilSleeping(for: interval, "the successful reading never waited")
    #expect(viewer.reading.picture == shown)
    #expect(viewer.reading.frame?.id == frame.id)
    #expect(viewer.reading.unavailable == nil)
    #expect(viewer.state.screen.picture == shown)
    #expect(viewer.state.screen.empty == nil)
    #expect(ViewerPresenter.status(viewer.snapshot).label == "Working")
    #expect(viewer.session.paused)
    #expect(service.frameCalls == frames)
  }

  @Test(
    "a live picture failure followed by pausing in Settings shows the next status and offers Take Control without fetching frames"
  )
  func aLivePictureFailureFollowedByPausingInSettingsShowsTheNextStatus() async throws {
    let service = ScriptedService()
    service.reply = ["ready": true, "canObserve": true, "overview": ["mode": "observing", "state": "idle"]]
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(service, clock: clock)
    defer { viewer.close() }
    let interval = Duration.seconds(ViewerPictureRate.live.seconds)
    try await clock.untilSleeping(for: interval, "the live reading never waited")

    service.fail = ["frame"]
    clock.advance(by: interval)
    try await settle("the live picture never failed") { viewer.reading.unavailable != nil }
    try #require(!viewer.reading.ready)
    try #require(viewer.reading.frame == nil)
    let frames = service.frameCalls
    let asked = service.statusCalls
    SettingsLinkRules.choose(.paused, viewer: viewer, preferences: .memory())
    try await settle("the paused viewer never read status") { service.statusCalls > asked }
    try await clock.untilSleeping(for: interval, "the paused reading never waited")

    service.fail = []
    service.reply = ["ready": true, "canObserve": true, "overview": ["mode": "agent", "state": "working"]]
    clock.advance(by: interval)
    try await clock.untilSleeping(for: interval, "the recovered reading never waited")
    #expect(viewer.reading.unavailable == nil)
    #expect(ViewerPresenter.status(viewer.snapshot).label == "Working")
    #expect(viewer.state.footer.controls.control?.event == .takeControl)
    #expect(viewer.state.footer.controls.control?.isEnabled == true)
    #expect(viewer.snapshot.canTakeControlOnceResumed)
    #expect(!viewer.snapshot.canTakeControl)
    #expect(!viewer.reading.ready)
    #expect(viewer.reading.frame == nil)
    #expect(viewer.reading.picture == nil)
    #expect(viewer.session.paused)
    #expect(service.frameCalls == frames)

    service.reply = [
      "ready": false, "canObserve": false,
      "overview": ["mode": "unavailable", "state": "notReady", "reason": "Atlas isn't answering in the helper."],
    ]
    clock.advance(by: interval)
    try await clock.untilSleeping(for: interval, "the unobservable reading never waited")
    #expect(viewer.reading.status.unobservable != nil)
    #expect(viewer.snapshot.mode.kind == .notReady)
    #expect(ViewerPresenter.status(viewer.snapshot).line == "Atlas isn't answering in the helper.")
    #expect(viewer.state.footer.controls.control == nil)
    #expect(service.frameCalls == frames)
  }

  @Test("the panel's picture shows at once, but control waits for a reading of its own")
  func thePanelsPictureShowsAtOnceButControlWaitsForAReading() async throws {
    let service = ScriptedService()
    service.picture = png()
    let viewer = ViewerController.scripted(service)
    let answer = try status(["ready": true, "overview": ["mode": "observing"]])
    let frame = Frame(png: png(), id: "panel", revision: "panel", display: ViewerStatus.Size(width: 4, height: 3))
    viewer.readingLoop.begin(with: answer, picture: DesktopPicture(frame: frame))
    #expect(viewer.reading.picture != nil)
    #expect(!viewer.snapshot.canTakeControl, "a picture from the panel names no frame")
    viewer.setVisible(true)
    try await settle("the window's own first picture never arrived") { viewer.reading.frame != nil }
    #expect(viewer.snapshot.canTakeControl)
    viewer.close()
  }

  @Test("a failure that arrives late never overwrites a paused viewer")
  func aLateFailureCannotOverwriteAPausedViewer() async throws {
    let service = ScriptedService()
    service.hold = ["status"]
    service.fail = ["status"]
    let viewer = ViewerController.scripted(service)
    viewer.setVisible(true)
    try await settle("the first reading never started") { service.held["status"] != nil }
    viewer.togglePause()
    service.resume("status")
    try await settle("the held reading never ended") { service.answered == 1 }
    #expect(viewer.reading.unavailable == nil)
    viewer.close()
  }

  @Test("a status that cannot be observed shows why in place of a picture")
  func aStatusThatCannotObserveSaysWhy() async throws {
    let service = ScriptedService()
    service.reply = [
      "canObserve": false, "nextSteps": ["Allow Screen Recording."], "overview": ["mode": "blocked"],
    ]
    let viewer = ViewerController.scripted(service)
    viewer.setVisible(true)
    try await settle("the picture was never found unavailable") { viewer.reading.unavailable != nil }
    #expect(viewer.reading.unavailable == viewer.reading.status.unobservable?.word)
    #expect(viewer.reading.frame == nil)
    viewer.close()
  }

  @Test("a status the service refuses to give, as a busy one does, leaves the picture and status as they were")
  func aRefusedStatusChangesNothing() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    let before = viewer.reading.status
    service.busy = true
    await viewer.readingLoop.read(viewer.readingLoop.generation)
    #expect(viewer.reading.unavailable == nil)
    #expect(viewer.reading.frame != nil)
    #expect(viewer.reading.status == before)
    viewer.close()
  }

  @Test("a picture that does not decode reads as a screen of unknown size")
  func anUndecodablePictureIsUnknownSize() async throws {
    let service = ScriptedService()
    service.picture = Data("not a png".utf8)
    let viewer = ViewerController.scripted(service)
    viewer.setVisible(true)
    try await settle("the picture was never found unavailable") { viewer.reading.unavailable != nil }
    #expect(viewer.reading.unavailable == ServiceError.unknownScreenSize.message)
    viewer.close()
  }

  @Test("a picture decoded after its reading was superseded is never shown", arguments: [false, true])
  func aSupersededDecodeIsNeverShown(stopped: Bool) async throws {
    let viewer = try await ViewerController.watching(ScriptedService())
    defer { viewer.close() }
    let shown = viewer.reading.picture
    let frame = viewer.reading.frame
    let started = ReleaseSignal()
    let finish = ReleaseSignal()
    defer {
      started.open()
      finish.open()
    }
    let reading = Task {
      await viewer.readingLoop.read(viewer.readingLoop.generation) { frame in
        let decoded = await DesktopPicture.decode(frame)
        started.open()
        await finish.wait()
        return decoded
      }
    }
    let waitingForDecode = Task { await started.wait() }
    try await waitingForDecode.value("the picture decode never started", within: Ceiling.work)
    if stopped {
      viewer.readingLoop.watch(false)
    } else {
      await viewer.readingLoop.readStatus()
    }
    finish.open()
    try await reading.value("the superseded reading never ended", within: Ceiling.work)
    #expect(viewer.reading.picture == shown)
    #expect(viewer.reading.frame?.id == frame?.id)
    #expect(viewer.reading.unavailable == nil)
  }

  @Test("a viewer reading waits one read interval on the injected clock before reading again")
  func aReadingWaitsOneIntervalBeforeTheNext() async throws {
    let service = ScriptedService()
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(service, clock: clock)
    // The clock's tick stops, so the reading's pause is the clock's one sleeper.
    viewer.readingLoop.ticking?.cancel()
    let interval = Duration.seconds(ViewerPictureRate.live.seconds)
    try await clock.untilSleeping(for: interval, "the reading never paused")
    #expect(clock.waiting == [interval])
    let asked = service.statusCalls
    clock.advance(by: interval - .milliseconds(1))
    #expect(clock.waiting == [.milliseconds(1)], "the pause has not ended a moment before its interval")
    clock.advance(by: .milliseconds(1))
    try await settle("the next reading never came") { service.statusCalls == asked + 1 }
    viewer.close()
  }

  @Test("the clock's tick moves the picture's age without redrawing the window")
  func theTickRedrawsNothing() async throws {
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(ScriptedService(), clock: clock)
    // Readings stop, so no new picture arrives as the time passes; the tick is read by hand below.
    viewer.readingLoop.watch(false)
    let window = OSAllocatedUnfairLock(initialState: false)
    withObservationTracking {
      _ = viewer.state
    } onChange: {
      window.withLock { $0 = true }
    }
    clock.advance(by: .seconds(10))
    viewer.readingLoop.readClock()
    #expect(ViewerPresenter.newest(viewer.snapshot) == "10 s ago", "the next drawing tells the new age")
    #expect(!window.withLock { $0 }, "the window stays as it was")
    viewer.close()
  }

  @Test("a saved rate that is not offered falls back to Live")
  func anUnofferedRateFallsBack() {
    #expect(ViewerController.scripted(ScriptedService(), rate: 3).readingLoop.rate == .live)
    #expect(ViewerController.scripted(ScriptedService(), rate: 5).readingLoop.rate == .everyFiveSeconds)
  }

  @Test("Copy Picture never asks the helper for a new frame")
  func copyPictureUsesTheFrameOnScreen() async throws {
    let service = ScriptedService()
    let pictures = RecordedPictures()
    let viewer = try await ViewerController.watching(
      service,
      pictures: pictures.outlet
    )
    let asked = service.frameCalls
    let shown = try #require(viewer.reading.picture, "a picture shows")
    viewer.readingLoop.copyPicture()
    #expect(service.frameCalls == asked)
    #expect(pictures.copied.count == 1)
    #expect(pictures.copied.first === shown.image)
    viewer.close()
  }

  @Test("the reading loop stops when the window closes, and its control is released first")
  func closingReleasesControlThenStopsReading() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.control.takeControl()
    try #require(viewer.control.lease == "lease")
    #expect(viewer.readingLoop.watching)
    let watchingAtRelease = OSAllocatedUnfairLock<Bool?>(initialState: nil)
    withObservationTracking {
      _ = viewer.control.lease
    } onChange: {
      // Called as the lease changes, on the main actor that changes it.
      let watching = MainActor.assumeIsolated { viewer.readingLoop.watching }
      watchingAtRelease.withLock { $0 = watching }
    }
    viewer.close()
    #expect(watchingAtRelease.withLock { $0 } == true, "control went back while the loop still read")
    #expect(!viewer.readingLoop.watching)
    #expect(viewer.readingLoop.polling == nil)
    #expect(viewer.readingLoop.ticking == nil)
    try await settle("the release never reached the service") {
      service.log.contains("release lease") && !viewer.barrier.isBusy
    }
  }

  @Test("connecting keeps the last picture and shows the ring only after 0.4 seconds")
  func connectingKeepsTheLastPictureAndRingsOnlyAfterADelay() async throws {
    let service = ScriptedService()
    service.reply = ["ready": true, "canObserve": true, "overview": ["mode": "observing", "state": "idle"]]
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(service, clock: clock)
    let shown = try #require(viewer.reading.picture)
    // Details' tick stops, so the clock's sleepers are the reading's pause and the wait for a picture.
    viewer.readingLoop.ticking?.cancel()
    #expect(viewer.state.screen.empty == nil)
    service.fail = ["status"]
    await viewer.readingLoop.read(viewer.readingLoop.generation)
    // The picture is lost: the stage keeps it behind Reconnecting…, and no ring yet.
    let waiting = try #require(viewer.state.screen.empty)
    #expect(viewer.state.screen.picture == nil)
    #expect(viewer.snapshot.mode.kind == .connecting)
    #expect(waiting.lastPicture == shown)
    #expect(waiting.message.headline == ViewerWords.reconnectingHeadline)
    #expect(!waiting.showsRing)
    // 0.4 seconds, as the window's clock reads it.
    let delay = Duration.seconds(Motion.Duration.appearDelay.seconds)
    #expect(abs(Motion.Duration.appearDelay.seconds - 0.4) < 1e-9)
    try await clock.untilSleeping(for: delay, "the wait for a picture never started")
    clock.advance(by: delay - .milliseconds(1))
    #expect(clock.waiting.contains(.milliseconds(1)), "the wait has a moment left")
    #expect(viewer.state.screen.empty?.showsRing == false)
    clock.advance(by: .milliseconds(1))
    try await settle("the ring never showed") { viewer.state.screen.empty?.showsRing == true }
    #expect(viewer.state.screen.empty?.lastPicture == shown, "the last picture stays as the ring arrives")
    viewer.close()
  }

  @Test("a helper that goes silent after a picture keeps that picture behind Not ready and Open Settings")
  func aSilentHelperKeepsTheLastPictureBehindNotReady() async throws {
    let service = ScriptedService()
    service.reply = ["ready": true, "canObserve": true, "overview": ["mode": "observing", "state": "idle"]]
    let viewer = try await ViewerController.watching(service)
    let shown = try #require(viewer.reading.picture)
    // A restarting helper: the service answers, saying the helper is silent.
    service.reply = [
      "ready": false, "canObserve": false,
      "overview": ["mode": "unavailable", "state": "notReady", "reason": "Atlas isn't answering in the helper."],
    ]
    await viewer.readingLoop.read(viewer.readingLoop.generation)
    let waiting = try #require(viewer.state.screen.empty)
    #expect(viewer.state.screen.picture == nil)
    #expect(waiting.message.headline == ViewerWords.notReadyLine(viewer.reading.status.overview.reason))
    #expect(waiting.event == .openSettings)
    #expect(waiting.lastPicture == shown, "the stage went blank")
    viewer.close()
  }

  @Test("stopping reading puts the ring away, so reading again waits its whole 0.4 seconds before it rings")
  func stoppingReadingStartsTheWaitAgain() async throws {
    let service = ScriptedService()
    service.reply = ["ready": true, "canObserve": true, "overview": ["mode": "observing", "state": "idle"]]
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(service, clock: clock)
    viewer.readingLoop.ticking?.cancel()
    service.fail = ["status"]
    await viewer.readingLoop.read(viewer.readingLoop.generation)
    let delay = Duration.seconds(Motion.Duration.appearDelay.seconds)
    try await clock.untilSleeping(for: delay, "the wait for a picture never started")
    clock.advance(by: delay)
    try await settle("the ring never showed") { viewer.reading.waitedLong }
    // Hidden or paused, then shown again with still no picture: the ring waits its moment again.
    viewer.readingLoop.watch(false)
    #expect(!viewer.reading.waitedLong, "the ring stayed out while reading stopped")
    viewer.readingLoop.watch(true)
    try await clock.untilSleeping(for: delay, "the wait never started again")
    #expect(!viewer.reading.waitedLong, "the ring showed at once")
    // Stopped before its moment ends, the wait starts over rather than ringing early.
    clock.advance(by: delay / 2)
    viewer.readingLoop.watch(false)
    viewer.readingLoop.watch(true)
    try await clock.untilSleeping(for: delay, "the wait never started over")
    clock.advance(by: delay / 2)
    #expect(!viewer.reading.waitedLong, "the ring kept the stopped wait's time")
    clock.advance(by: delay / 2)
    try await settle("the ring never showed after its whole moment") { viewer.reading.waitedLong }
    viewer.close()
  }
}
