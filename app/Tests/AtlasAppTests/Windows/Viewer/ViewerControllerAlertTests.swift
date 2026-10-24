import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("ViewerController+Alert")
struct ViewerControllerAlertTests {
  @Test("the alert's Recover runs once however many times it is pressed")
  func recoverRunsOnce() async throws {
    let service = ScriptedService()
    service.reply = [
      "ready": true, "canObserve": true, "overview": ["mode": "recovering", "state": "needsYou", "pending": 1],
    ]
    let commands = ScriptedCommands(["restart": #"{"ok": true}"#])
    let release = ReleaseSignal()
    let running = CallCount()
    commands.whileRunning { _ in
      running.count += 1
      await release.wait()
    }
    let viewer = try await ViewerController.watching(service, commands: commands)
    try #require(viewer.state.alert != nil, "the recovery alert shows")
    viewer.send(.alertConfirm)
    viewer.send(.alertConfirm)
    try await settle("Recover never ran") { running.count == 1 }
    viewer.send(.alertConfirm)
    #expect(viewer.state.alert?.isBusy == true, "its buttons wait while it runs")
    // The third press reaches the controller while the command still waits, before it is let go.
    await afterQueuedWork()
    release.open()
    try await settle("Recover never ended") { !viewer.session.recovering }
    #expect(commands.ran == [Action.restart.arguments])
    viewer.close()
  }
}
