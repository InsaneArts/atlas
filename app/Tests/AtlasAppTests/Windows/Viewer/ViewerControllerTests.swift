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

  @Test("recovering clears a recovery set aside, restarts the helper an unknown outcome left, and says so")
  func recoveringSaysSo() async throws {
    let service = ScriptedService()
    service.reply = [
      "ready": true, "canObserve": true,
      "overview": ["mode": "recovering", "reason": "Unknown outcome", "pending": 1],
    ]
    let commands = ScriptedCommands(["restart": #"{"ok": true}"#])
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(service, commands: commands, clock: clock)
    viewer.cancelAlert()
    #expect(viewer.snapshot.recovery == .setAside)
    let recovering = Task { await viewer.recover() }
    let hold = Duration.seconds(Motion.Duration.doneHold.seconds)
    try await clock.untilSleeping(for: hold, "Recover never held on Recovered")
    clock.advance(by: hold)
    await recovering.value
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
    await viewer.readingLoop.readStatus()
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

  @Test("Switch to the Helper… shows in the window what locking the screen said, a failure included")
  func switchingSaysHowItWent() async {
    let viewer = ViewerController.scripted(ScriptedService(), switchToHelper: { "The screen could not lock" })
    viewer.send(.askSwitch)
    await viewer.confirmAlert()
    #expect(viewer.session.notice == "The screen could not lock")
    #expect(viewer.state.alert == nil)
  }

  @Test("Details reads the connected agents when it opens and by Check Again, never with each reading")
  func detailsReadsAgentsWhenItOpens() async throws {
    let service = ScriptedService()
    let reads = CallCount()
    let viewer = ViewerController.scripted(
      service,
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
    try await settle("the agents were never read") { viewer.details.agents == [.codex] }
    await viewer.readingLoop.read(viewer.readingLoop.generation)
    #expect(reads.count == 1, "a reading leaves them be")
    await viewer.checkAgain()
    #expect(reads.count == 2)
    #expect(viewer.state.details?.session.map(\.value) == ["Codex"])
    viewer.close()
  }
}
