import AsyncSupportTesting
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("ViewerController+Tabs")
struct ViewerControllerTabsTests {
  @Test("leaving the Screen tab gives control back")
  func leavingTheScreenTabGivesControlBack() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.takeControl()
    try #require(viewer.session.lease == "lease", "control was taken")
    viewer.send(.selectTab(.activity))
    #expect(viewer.session.lease == nil)
    #expect(viewer.state.top.tabs.selected == .activity)
    try await settle("the lease was never released") { service.log.contains("release lease") }
    viewer.close()
  }

  @Test("switching tabs while a release is unanswered keeps Take Control waiting until it is answered")
  func takeControlWaitsForTheRelease() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.takeControl()
    service.hold = ["release"]
    viewer.send(.selectTab(.apps))
    try await settle("the release was never sent") { service.held["release"] != nil }
    viewer.send(.selectTab(.screen))
    #expect(!viewer.state.top.actions.control.isEnabled)
    await viewer.takeControl()
    #expect(service.log.filter { $0.hasPrefix("acquire") }.count == 1, "nothing is asked before the answer")
    service.resume("release")
    try await settle("Take Control never came back") { viewer.state.top.actions.control.isEnabled }
    viewer.close()
  }
}
