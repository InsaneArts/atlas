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

  @Test("Stop Agent and Resume Agent ask the service and say how it went")
  func stoppingAndResumingTheAgent() async throws {
    let service = ScriptedService()
    let viewer = ViewerController.scripted(service)
    await viewer.setStopped(true)
    #expect(service.log == ["stop"])
    #expect(viewer.session.notice == ViewerWords.agentStopped)
    await viewer.setStopped(false)
    #expect(service.log == ["stop", "resume"])
    #expect(viewer.session.notice == ViewerWords.agentResumed)
    service.fail = ["hold"]
    await viewer.setStopped(true)
    #expect(viewer.session.notice == "hold failed", "a refusal says why")
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
    #expect(viewer.reading.unavailable == viewer.reading.status.unobservable?.word)
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

  @Test("recovering clears a recovery set aside, restarts the helper an unknown outcome left, and says so")
  func recoveringSaysSo() async throws {
    let service = ScriptedService()
    service.reply = [
      "ready": true, "canObserve": true,
      "overview": ["mode": "recovering", "reason": "Unknown outcome", "pending": 1],
    ]
    let commands = ScriptedCommands(["restart": #"{"ok": true}"#])
    let viewer = try await ViewerController.watching(service, commands: commands)
    viewer.cancelAlert()
    #expect(viewer.snapshot.recovery == .setAside)
    await viewer.recover()
    #expect(commands.ran == [Action.restart.arguments])
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
    viewer.cancelAlert()
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
      _ = viewer.state.alert
    } onChange: {
      redrawn.withLock { $0 = true }
    }
    viewer.cancelAlert()
    #expect(redrawn.withLock { $0 }, "the window redraws without waiting for a reading")
    viewer.close()
  }

  @Test("the clock's tick moves the picture's age in Details, and redraws nothing else")
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
      _ = viewer.newestPicture
    } onChange: {
      line.withLock { $0 = true }
    }
    clock.now = 10
    viewer.readClock()
    #expect(line.withLock { $0 }, "the age row draws again")
    #expect(viewer.newestPicture == "10 s ago")
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

  @Test("Switch to the Helper… shows in the window what locking the screen said, a failure included")
  func switchingSaysHowItWent() async {
    let viewer = ViewerController.scripted(ScriptedService(), switchToHelper: { "The screen could not lock" })
    viewer.send(.askSwitch)
    await viewer.confirmAlert()
    #expect(viewer.session.notice == "The screen could not lock")
    #expect(viewer.state.alert == nil)
  }

  @Test("a saved rate that is not offered falls back to Live")
  func anUnofferedRateFallsBack() {
    #expect(ViewerController.scripted(ScriptedService(), rate: 3).rate == .live)
    #expect(ViewerController.scripted(ScriptedService(), rate: 5).rate == .everyFiveSeconds)
  }

  @Test("Details reads the connected agents when it opens and by Check Again, never with each reading")
  func detailsReadsAgentsWhenItOpens() async throws {
    let service = ScriptedService()
    let reads = CallCount()
    let viewer = ViewerController.scripted(
      service,
      nap: ViewerController.untilCancelled,
      connectedAgents: {
        reads.count += 1
        return [AgentID.codex]
      }
    )
    service.picture = png()
    viewer.setVisible(true)
    try await settle("the first picture never arrived") { viewer.reading.frame != nil }
    #expect(reads.count == 0, "closed Details reads nothing")
    viewer.send(.toggleDetails)
    try await settle("the agents were never read") { viewer.agents == [.codex] }
    await viewer.read(viewer.generation)
    #expect(reads.count == 1, "a reading leaves them be")
    await viewer.checkAgain()
    #expect(reads.count == 2)
    #expect(viewer.state.details?.session.map(\.value) == ["Codex"])
    viewer.close()
  }
}
