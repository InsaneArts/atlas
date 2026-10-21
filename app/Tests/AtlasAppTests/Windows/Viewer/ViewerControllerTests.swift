import AtlasKit
import AtlasUI
import Foundation
import Observation
import Testing
import os

@testable import AtlasApp

@MainActor
@Suite("ViewerController")
struct ViewerControllerTests {
  @Test("a reading shows the status and the picture, and control can be taken")
  func aReadingShowsTheStatusAndThePicture() async throws {
    let viewer = try await ViewerController.watching(ScriptedService())
    #expect(viewer.reading.frame != nil)
    #expect(viewer.reading.picture != nil)
    #expect(viewer.reading.ready)
    #expect(viewer.snapshot.canTakeControl)
    viewer.close()
  }

  @Test("the panel's picture shows at once, but control waits for a reading of its own")
  func thePanelsPictureShowsAtOnceButControlWaitsForAReading() async throws {
    let service = ScriptedService()
    service.picture = png()
    let viewer = ViewerController.scripted(service)
    let answer = try status(["ready": true, "overview": ["mode": "observing"]])
    let frame = Frame(png: png(), id: "panel", revision: "panel", display: ViewerStatus.Size(width: 4, height: 3))
    viewer.begin(with: answer, picture: DesktopPicture(frame: frame))
    #expect(viewer.reading.picture != nil)
    #expect(!viewer.snapshot.canTakeControl, "a picture from the panel names no frame")
    viewer.setVisible(true)
    try await settle("the window's own first picture never arrived") { viewer.reading.frame != nil }
    #expect(viewer.snapshot.canTakeControl)
    viewer.close()
  }

  @Test("pausing drops a reading under way, and resuming reads once")
  func pausingDropsAReadingStillUnderWayAndResumingReadsOnce() async throws {
    let service = ScriptedService()
    service.hold = ["status"]
    service.picture = png()
    let viewer = ViewerController.scripted(service)
    viewer.setVisible(true)
    try await settle("the first reading never started") { service.held["status"] != nil }
    viewer.togglePause()
    viewer.togglePause()
    viewer.togglePause()
    service.resume("status")
    try await settle("the held reading never ended") { service.answered == 1 }
    #expect(viewer.reading.frame == nil, "the paused window shows no late picture")
    #expect(viewer.session.paused)
    viewer.togglePause()
    try await settle("no picture arrived after resuming") { viewer.reading.frame != nil }
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
    #expect(viewer.reading.unavailable == viewer.reading.status.unobservableReason)
    #expect(viewer.reading.frame == nil)
    viewer.close()
  }

  @Test("a status the service refuses to give, as a busy one does, leaves the picture and status as they were")
  func aRefusedStatusChangesNothing() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service, nap: ViewerController.untilCancelled)
    let before = viewer.reading.status
    service.busy = true
    await viewer.read(viewer.generation)
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

  @Test("recovering clears a recovery set aside and says the helper was recovered")
  func recoveringSaysSo() async throws {
    let service = ScriptedService()
    service.reply = [
      "ready": true, "canObserve": true, "overview": ["mode": "recovering", "reason": "Unknown outcome"],
    ]
    let viewer = try await ViewerController.watching(service)
    viewer.recoverLater()
    #expect(viewer.snapshot.recovery == .setAside)
    await viewer.recover()
    #expect(service.log.contains("recover"))
    #expect(viewer.session.recoveryLater == nil)
    #expect(viewer.session.notice == Words.recovered)
    viewer.close()
  }

  @Test("a recovery set aside is forgotten once nothing needs it, so a later one alerts again")
  func aRecoverySetAsideIsForgotten() async throws {
    let service = ScriptedService()
    service.reply = [
      "ready": true, "canObserve": true, "overview": ["mode": "recovering", "reason": "Unknown outcome"],
    ]
    let viewer = try await ViewerController.watching(service)
    viewer.recoverLater()
    #expect(viewer.snapshot.recovery == .setAside)
    service.reply = ["ready": true, "canObserve": true, "overview": ["mode": "observing"]]
    await viewer.readStatus()
    #expect(viewer.session.recoveryLater == nil)
    viewer.close()
  }

  @Test("Not Now dismisses the recovery alert at once, not at the next reading")
  func notNowDismissesAtOnce() async throws {
    let service = ScriptedService()
    service.reply = [
      "ready": true, "canObserve": true, "overview": ["mode": "recovering", "reason": "Unknown outcome"],
    ]
    let viewer = try await ViewerController.watching(service)
    let redrawn = OSAllocatedUnfairLock(initialState: false)
    withObservationTracking {
      _ = viewer.state.recovery
    } onChange: {
      redrawn.withLock { $0 = true }
    }
    viewer.recoverLater()
    #expect(redrawn.withLock { $0 }, "the window redraws without waiting for a reading")
    viewer.close()
  }

  @Test("the clock's tick moves the picture's age in the status line, and redraws nothing else")
  func theTickRedrawsTheStatusLineAlone() async throws {
    let clock = TestClock()
    let viewer = try await ViewerController.watching(ScriptedService(), clock: clock.read)
    let window = OSAllocatedUnfairLock(initialState: false)
    let line = OSAllocatedUnfairLock(initialState: false)
    withObservationTracking {
      _ = viewer.state
    } onChange: {
      window.withLock { $0 = true }
    }
    withObservationTracking {
      _ = viewer.headerDetail
    } onChange: {
      line.withLock { $0 = true }
    }
    clock.now = 4
    viewer.readClock()
    #expect(line.withLock { $0 }, "the status line draws again")
    #expect(viewer.headerDetail == ViewerWords.headerDetail(account: ViewerWords.helper, freshness: "Updated 4 s ago"))
    #expect(!window.withLock { $0 }, "the rest of the window stays as it was")
    viewer.close()
  }

  @Test("the same composed text again changes nothing the window draws")
  func composingTheSameTextChangesNothing() {
    let viewer = ViewerController.scripted(ScriptedService())
    viewer.compose("ni")
    let redrawn = OSAllocatedUnfairLock(initialState: false)
    withObservationTracking {
      _ = viewer.state
    } onChange: {
      redrawn.withLock { $0 = true }
    }
    viewer.compose("ni")
    #expect(!redrawn.withLock { $0 })
  }

  @Test("Log In… shows in the viewer what locking the screen said, a failure included")
  func logInSaysHowItWent() async {
    let viewer = ViewerController(
      service: ScriptedService(),
      preferences: .memory(rate: 0),
      clock: { 0 },
      heartbeatInterval: ViewerController.heartbeatEvery,
      nap: ViewerController.sleep,
      logIn: { "The screen could not lock" },
      showKeys: { _ in },
      doubleClickInterval: { 0.5 }
    )
    await viewer.askToLogIn()
    #expect(viewer.session.notice == "The screen could not lock")
  }

  @Test("a saved rate that is not offered falls back to every 2 seconds")
  func anUnofferedRateFallsBack() {
    #expect(ViewerController.scripted(ScriptedService(), rate: 3).rate == .everyTwoSeconds)
    #expect(ViewerController.scripted(ScriptedService(), rate: 5).rate == .everyFiveSeconds)
  }
}
