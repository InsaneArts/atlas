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
    await viewer.control.takeControl()
    try #require(viewer.control.lease == "lease", "control was taken")
    viewer.send(.selectTab(.activity))
    #expect(viewer.control.lease == nil)
    #expect(viewer.state.top.tabs.selected == .activity)
    try await settle("the lease was never released") { service.log.contains("release lease") }
    viewer.close()
  }

  @Test("switching tabs while a release is unanswered keeps Take Control waiting until it is answered")
  func takeControlWaitsForTheRelease() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.control.takeControl()
    service.hold = ["release"]
    viewer.send(.selectTab(.apps))
    try await settle("the release was never sent") { service.held["release"] != nil }
    viewer.send(.selectTab(.screen))
    #expect(viewer.state.top.actions.control?.isEnabled != true)
    await viewer.control.takeControl()
    #expect(service.log.filter { $0.hasPrefix("acquire") }.count == 1, "nothing is asked before the answer")
    service.resume("release")
    try await settle("Take Control never came back") { viewer.state.top.actions.control?.isEnabled == true }
    viewer.close()
  }

  @Test("the unseen dot turns orange when an unseen action failed, and clears when Activity is chosen")
  func theUnseenDotFollowsWhatWasSeen() async throws {
    let service = ScriptedService()
    service.reply = Self.reply([["id": "a1", "operation": "input", "ok": true, "ageSeconds": 30]])
    let viewer = try await ViewerController.watching(service)
    #expect(viewer.state.top.activityUnseen == .none, "what happened before the window opened is not new")
    service.reply = Self.reply([
      ["id": "a2", "operation": "input", "ok": true, "ageSeconds": 1],
      ["id": "a1", "operation": "input", "ok": true, "ageSeconds": 31],
    ])
    try await Self.read(viewer, until: "a2")
    #expect(viewer.state.top.activityUnseen == .new)
    service.reply = Self.reply([
      ["id": "a3", "operation": "input", "ok": false, "ageSeconds": 1],
      ["id": "a2", "operation": "input", "ok": true, "ageSeconds": 2],
      ["id": "a1", "operation": "input", "ok": true, "ageSeconds": 32],
    ])
    try await Self.read(viewer, until: "a3")
    #expect(viewer.state.top.activityUnseen == .failed, "one unseen action failed")
    viewer.send(.selectTab(.activity))
    #expect(viewer.state.top.activityUnseen == .none, "Activity shows them")
    viewer.send(.selectTab(.screen))
    #expect(viewer.state.top.activityUnseen == .none, "seen stays seen")
    viewer.close()
  }

  /// A status with `activity`, newest first.
  ///
  /// - Parameter activity: The helper's recent actions.
  /// - Returns: The reply.
  static func reply(_ activity: [[String: Any]]) -> [String: Any] {
    ["ready": true, "canObserve": true, "overview": ["mode": "observing", "state": "idle", "activity": activity]]
  }

  /// Reads the status at once until its newest action is `newest`.
  ///
  /// - Parameters:
  ///   - viewer: The viewer.
  ///   - newest: The newest action's id.
  /// - Throws: The failed requirement, when it never shows.
  static func read(_ viewer: ViewerController, until newest: String) async throws {
    viewer.readingLoop.refresh()
    try await settle("\(newest) never showed") {
      viewer.reading.statusReading.activity(limit: 1).first?.id == newest
    }
  }
}
