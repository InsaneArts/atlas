import AsyncSupportTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("ViewerController+Places")
struct ViewerControllerPlacesTests {
  /// A session the scripted service lists.
  static let key = "3f9a1c07b2e4"

  @Test("choosing a session from the Screen opens its Activity")
  func choosingASessionOpensItsActivity() async throws {
    let viewer = try await ViewerController.watching(ScriptedService())
    viewer.send(.chooseSession(.one(Self.key)))
    #expect(viewer.session.chosenSession == .one(Self.key))
    #expect(viewer.state.pane.place == .activity)
    viewer.close()
  }

  @Test("choosing all sessions from the Screen opens Activity for every session")
  func choosingAllSessionsOpensActivity() async throws {
    let viewer = try await ViewerController.watching(ScriptedService())
    viewer.send(.chooseSession(.all))
    #expect(viewer.session.chosenSession == .all)
    #expect(viewer.state.pane.place == .activity)
    viewer.close()
  }

  @Test("choosing the chosen session again from the Screen opens its Activity")
  func choosingTheChosenSessionAgainOpensItsActivity() async throws {
    let viewer = try await ViewerController.watching(ScriptedService())
    viewer.send(.chooseSession(.one(Self.key)))
    viewer.send(.show(.screen))
    try #require(viewer.state.pane.place == .screen)
    viewer.send(.chooseSession(.one(Self.key)))
    #expect(viewer.state.pane.place == .activity)
    viewer.close()
  }

  @Test("choosing a session while Activity shows narrows it and stays on Activity")
  func choosingASessionOnActivityNarrowsIt() async throws {
    let viewer = try await ViewerController.watching(ScriptedService())
    viewer.send(.chooseSession(.one(Self.key)))
    viewer.send(.chooseSession(.all))
    #expect(viewer.session.chosenSession == .all)
    #expect(viewer.state.pane.place == .activity)
    viewer.close()
  }

  @Test("pressing the corner screen goes back to the Screen")
  func pressingTheCornerScreenGoesBack() async throws {
    let viewer = try await ViewerController.watching(ScriptedService())
    viewer.send(.chooseSession(.all))
    try #require(viewer.state.pane.place == .activity)
    // The corner screen's click sends the Screen, as the view does.
    viewer.send(.show(.screen))
    #expect(viewer.state.pane.place == .screen)
    viewer.close()
  }

  @Test("no session row is chosen while the Screen shows, and the chosen one is while Activity shows")
  func theSelectionShowsOnlyOnActivity() async throws {
    let viewer = try await ViewerController.watching(ScriptedService())
    #expect(viewer.state.sidebar.sessions.chosen == nil)
    viewer.send(.chooseSession(.one(Self.key)))
    #expect(viewer.state.sidebar.sessions.chosen != nil)
    #expect(viewer.state.sidebar.sessions.chosen == viewer.snapshot.shownSessions)
    viewer.send(.show(.screen))
    #expect(viewer.state.sidebar.sessions.chosen == nil)
    #expect(viewer.session.chosenSession == .one(Self.key), "the choice is kept for Activity")
    viewer.close()
  }

  @Test("a session or all sessions chosen while this window holds control keeps the Screen and control")
  func aSessionChosenInControlKeepsTheScreen() async throws {
    #expect(!ViewerController.opensActivity(from: .screen, holdingControl: true))
    #expect(ViewerController.opensActivity(from: .screen, holdingControl: false))
    #expect(!ViewerController.opensActivity(from: .activity, holdingControl: false))
    let viewer = try await ViewerController.watching(ScriptedService())
    await viewer.control.takeControl()
    try #require(viewer.control.lease == "lease", "control was taken")
    for choice in [ViewerSessionChoice.one(Self.key), .all] {
      viewer.send(.chooseSession(choice))
      #expect(viewer.state.pane.place == .screen, "\(choice)")
      #expect(viewer.control.lease == "lease", "\(choice)")
    }
    viewer.close()
  }

  @Test("leaving the Screen gives control back")
  func leavingTheScreenTabGivesControlBack() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.control.takeControl()
    try #require(viewer.control.lease == "lease", "control was taken")
    viewer.send(.show(.activity))
    #expect(viewer.control.lease == nil)
    #expect(viewer.state.pane.place == .activity)
    try await settle("the lease was never released") { service.log.contains("release lease") }
    viewer.close()
  }

  @Test("switching places while a release is unanswered keeps Take Control waiting until it is answered")
  func takeControlWaitsForTheRelease() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.control.takeControl()
    service.hold = ["release"]
    viewer.send(.show(.activity))
    try await settle("the release was never sent") { service.held["release"] != nil }
    viewer.send(.show(.screen))
    #expect(viewer.state.footer.controls.control?.isEnabled != true)
    await viewer.control.takeControl()
    #expect(service.log.filter { $0.hasPrefix("acquire") }.count == 1, "nothing is asked before the answer")
    service.resume("release")
    try await settle("Take Control never came back") { viewer.state.footer.controls.control?.isEnabled == true }
    viewer.close()
  }
}
