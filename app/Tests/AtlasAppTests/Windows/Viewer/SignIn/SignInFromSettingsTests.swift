import AsyncSupportTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("WindowCoordinator, a held sign-in started from Settings")
@MainActor
struct SignInFromSettingsTests {
  @Test(
    "a refused start leaves Settings in front with its refusal, whether or not the main window was open",
    arguments: [true, false]
  )
  func aRefusalLeavesSettingsInFront(viewerOpen: Bool) async throws {
    let service = ScriptedService()
    service.refusals["sign-in/start"] = ServiceError("An agent's command is still running.", code: "busy")
    let controller = try await ViewerController.watching(service)
    let parts = StandInParts()
    parts.makeViewer = { controller }
    let coordinator = parts.coordinator()
    if viewerOpen { coordinator.openViewer() }
    coordinator.openSettings()
    let settings = try #require(parts.front)
    let refusal = await coordinator.settingsLinks()
      .startSignIn(.signIn, "github.com", nil)
    #expect(refusal == "An agent's command is still running.")
    #expect(parts.front === settings, "Settings stays in front, its sheet saying why")
    #expect(controller.control.signInLease == nil)
    controller.close()
  }

  @Test("an open main window comes forward on the Screen only once the start succeeded")
  func theViewerComesForwardOnceStarted() async throws {
    let service = ScriptedService()
    let controller = try await ViewerController.watching(service)
    let parts = StandInParts()
    parts.makeViewer = { controller }
    let coordinator = parts.coordinator()
    coordinator.openViewer(place: .activity)
    let viewer = try #require(parts.front)
    coordinator.openSettings()
    let settings = try #require(parts.front)
    service.hold.insert("sign-in/start")
    let starting = Task {
      await coordinator.settingsLinks()
        .startSignIn(.signIn, "github.com", nil)
    }
    try await settle("the start never reached the service") { service.held["sign-in/start"] != nil }
    #expect(parts.front === settings, "Settings stays in front while the start is asked")
    service.resume("sign-in/start")
    #expect(await starting.value == nil)
    #expect(parts.front === viewer)
    #expect(controller.session.place == .screen)
    #expect(controller.control.signInLease == "sign-in-lease")
    controller.close()
  }
}
