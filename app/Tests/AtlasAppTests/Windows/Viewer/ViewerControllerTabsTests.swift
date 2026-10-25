import AsyncSupportTesting
import AtlasService
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

  @Test("an unseen failure keeps the dot orange after newer actions push it out of the status")
  func anUnseenFailureOutlastsTheStatusRows() async throws {
    let service = ScriptedService()
    service.reply = Self.reply([Self.row("a1", ok: true)])
    let viewer = try await ViewerController.watching(service)
    service.reply = Self.reply([Self.row("a2", ok: false), Self.row("a1", ok: true)])
    try await Self.read(viewer, until: "a2")
    #expect(viewer.state.top.activityUnseen == .failed)
    // Twelve newer actions that worked: the status lists twelve, so the failure has left it unseen.
    service.reply = Self.reply((1...12).reversed().map { Self.row("b\($0)", ok: true) })
    try await Self.read(viewer, until: "b12")
    #expect(viewer.state.top.activityUnseen == .failed, "the failure was never seen")
    viewer.send(.selectTab(.activity))
    viewer.send(.selectTab(.screen))
    #expect(viewer.state.top.activityUnseen == .none, "Activity showed it")
    viewer.close()
  }

  @Test("what Activity shows while the picture is paused is seen, so the dot stays off once it resumes")
  func activityReadWhilePausedIsSeen() async throws {
    let service = ScriptedService()
    service.reply = Self.reply([Self.row("a1", ok: true)])
    let viewer = try await ViewerController.watching(service)
    viewer.send(.togglePause)
    // The helper acts while the picture, and with it the status, is paused; Activity reads its own log meanwhile.
    let newer = [Self.row("a3", ok: false), Self.row("a2", ok: true), Self.row("a1", ok: true)]
    service.reply = Self.reply(newer)
    service.activityLog = ActivityLog(entries: [
      ActivityLog.Entry(id: "a3", outcome: .failed),
      ActivityLog.Entry(id: "a2"),
      ActivityLog.Entry(id: "a1"),
    ])
    viewer.send(.selectTab(.activity))
    try await settle("Activity never read its log") { viewer.panes.activity.log?.entries.first?.id == "a3" }
    viewer.send(.selectTab(.screen))
    viewer.send(.togglePause)
    try await Self.read(viewer, until: "a3")
    #expect(viewer.state.top.activityUnseen == .none, "Activity showed a2 and a3 before the status caught up")
    viewer.close()
  }

  /// One recent action in a status.
  ///
  /// - Parameters:
  ///   - id: Its id.
  ///   - ok: Whether it worked.
  /// - Returns: The row.
  static func row(_ id: String, ok: Bool) -> [String: Any] {
    ["id": id, "operation": "input", "ok": ok, "ageSeconds": 1]
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
