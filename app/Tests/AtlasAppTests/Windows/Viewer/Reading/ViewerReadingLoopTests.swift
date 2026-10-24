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

  @Test("a viewer reading waits one read interval on the injected clock before reading again")
  func aReadingWaitsOneIntervalBeforeTheNext() async throws {
    let service = ScriptedService()
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(service, clock: clock)
    // Details' tick stops, so the reading's pause is the clock's one sleeper.
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

  @Test("the clock's tick moves the picture's age in Details, and redraws nothing else")
  func theTickRedrawsTheStatusLineAlone() async throws {
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(ScriptedService(), clock: clock)
    // Readings stop, so no new picture arrives as the time passes; the tick is read by hand below.
    viewer.readingLoop.watch(false)
    let window = OSAllocatedUnfairLock(initialState: false)
    let line = OSAllocatedUnfairLock(initialState: false)
    withObservationTracking {
      _ = viewer.state
    } onChange: {
      window.withLock { $0 = true }
    }
    withObservationTracking {
      _ = viewer.newestPicture
    } onChange: {
      line.withLock { $0 = true }
    }
    clock.advance(by: .seconds(10))
    viewer.readingLoop.readClock()
    #expect(line.withLock { $0 }, "the age row draws again")
    #expect(viewer.newestPicture == "10 s ago")
    #expect(!window.withLock { $0 }, "the rest of the window stays as it was")
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
}
