import AsyncSupportTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("ViewerController+Places")
struct ViewerControllerPlacesTests {
  @Test("choosing the shown list again returns to the Screen")
  func choosingTheShownListAgainReturnsToTheScreen() async throws {
    let viewer = try await ViewerController.watching(ScriptedService())
    viewer.send(.chooseLibrary(.activity))
    #expect(viewer.state.pane.place == .activity)
    viewer.send(.chooseLibrary(.screenshots))
    #expect(viewer.state.pane.place == .screenshots, "another list is a step sideways")
    viewer.send(.chooseLibrary(.screenshots))
    #expect(viewer.state.pane.place == .screen)
    viewer.close()
  }

  @Test("choosing a session shows its activity, and choosing all sessions keeps the place shown")
  func choosingASessionShowsItsActivity() async throws {
    let viewer = try await ViewerController.watching(ScriptedService())
    viewer.send(.chooseSession(.one("3f9a1c07b2e4")))
    #expect(viewer.session.chosenSession == .one("3f9a1c07b2e4"))
    #expect(viewer.state.pane.place == .activity)
    viewer.send(.chooseLibrary(.screenshots))
    viewer.send(.chooseSession(.all))
    #expect(viewer.session.chosenSession == .all)
    #expect(viewer.state.pane.place == .screenshots, "every session's work shows where the window is")
    viewer.close()
  }

  @Test("choosing a session narrows the list that shows, and opens Activity only from the Screen")
  func choosingASessionNarrowsTheListThatShows() async throws {
    let viewer = try await ViewerController.watching(ScriptedService())
    viewer.send(.chooseLibrary(.screenshots))
    viewer.send(.chooseSession(.one("3f9a1c07b2e4")))
    #expect(viewer.session.chosenSession == .one("3f9a1c07b2e4"))
    #expect(viewer.state.pane.place == .screenshots, "the screenshots narrow to the session")
    viewer.send(.chooseLibrary(.apps))
    viewer.send(.chooseSession(.all))
    #expect(viewer.state.pane.place == .apps)
    viewer.close()
  }

  @Test("a session chosen while this window holds control keeps the Screen and control")
  func aSessionChosenInControlKeepsTheScreen() {
    #expect(!ViewerController.opensActivity(for: .one("3f9a1c07b2e4"), from: .screen, holdingControl: true))
    #expect(ViewerController.opensActivity(for: .one("3f9a1c07b2e4"), from: .screen, holdingControl: false))
    #expect(!ViewerController.opensActivity(for: .all, from: .screen, holdingControl: false))
    for place in [ViewerPlace.activity, .screenshots, .apps] {
      let opens = ViewerController.opensActivity(for: .one("3f9a1c07b2e4"), from: place, holdingControl: false)
      #expect(!opens, "\(place)")
    }
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

  @Test("switching tabs while a release is unanswered keeps Take Control waiting until it is answered")
  func takeControlWaitsForTheRelease() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.control.takeControl()
    service.hold = ["release"]
    viewer.send(.show(.apps))
    try await settle("the release was never sent") { service.held["release"] != nil }
    viewer.send(.show(.screen))
    #expect(viewer.state.footer.controls.control?.isEnabled != true)
    await viewer.control.takeControl()
    #expect(service.log.filter { $0.hasPrefix("acquire") }.count == 1, "nothing is asked before the answer")
    service.resume("release")
    try await settle("Take Control never came back") { viewer.state.footer.controls.control?.isEnabled == true }
    viewer.close()
  }

  @Test("the unseen dot turns orange when an unseen action failed, and clears when Activity is chosen")
  func theUnseenDotFollowsWhatWasSeen() async throws {
    let service = ScriptedService()
    service.reply = Self.reply([["id": "a1", "operation": "input", "ok": true, "ageSeconds": 30]])
    let viewer = try await ViewerController.watching(service)
    #expect(ViewerPresenter.unseen(viewer.snapshot) == .none, "what happened before the window opened is not new")
    service.reply = Self.reply([
      ["id": "a2", "operation": "input", "ok": true, "ageSeconds": 1],
      ["id": "a1", "operation": "input", "ok": true, "ageSeconds": 31],
    ])
    try await Self.read(viewer, until: "a2")
    #expect(ViewerPresenter.unseen(viewer.snapshot) == .new)
    service.reply = Self.reply([
      ["id": "a3", "operation": "input", "ok": false, "ageSeconds": 1],
      ["id": "a2", "operation": "input", "ok": true, "ageSeconds": 2],
      ["id": "a1", "operation": "input", "ok": true, "ageSeconds": 32],
    ])
    try await Self.read(viewer, until: "a3")
    #expect(ViewerPresenter.unseen(viewer.snapshot) == .failed, "one unseen action failed")
    viewer.send(.show(.activity))
    #expect(ViewerPresenter.unseen(viewer.snapshot) == .none, "Activity shows them")
    viewer.send(.show(.screen))
    #expect(ViewerPresenter.unseen(viewer.snapshot) == .none, "seen stays seen")
    viewer.close()
  }

  @Test("an unseen action refused before anything was sent is new, never orange")
  func aRefusedActionIsNeverOrange() async throws {
    let service = ScriptedService()
    service.reply = Self.reply([Self.row("a1", ok: true)])
    let viewer = try await ViewerController.watching(service)
    service.reply = Self.reply([
      ["id": "a2", "operation": "input", "ok": false, "nothingSent": true, "ageSeconds": 1],
      Self.row("a1", ok: true),
    ])
    try await Self.read(viewer, until: "a2")
    #expect(ViewerPresenter.unseen(viewer.snapshot) == .new)
    viewer.close()
  }

  @Test("an unseen failure keeps the dot orange after newer actions push it out of the status")
  func anUnseenFailureOutlastsTheStatusRows() async throws {
    let service = ScriptedService()
    service.reply = Self.reply([Self.row("a1", ok: true)])
    let viewer = try await ViewerController.watching(service)
    service.reply = Self.reply([Self.row("a2", ok: false), Self.row("a1", ok: true)])
    try await Self.read(viewer, until: "a2")
    #expect(ViewerPresenter.unseen(viewer.snapshot) == .failed)
    // Twelve newer actions that worked: the status lists twelve, so the failure has left it unseen.
    service.reply = Self.reply((1...12).reversed().map { Self.row("b\($0)", ok: true) })
    try await Self.read(viewer, until: "b12")
    #expect(ViewerPresenter.unseen(viewer.snapshot) == .failed, "the failure was never seen")
    viewer.send(.show(.activity))
    viewer.send(.show(.screen))
    #expect(ViewerPresenter.unseen(viewer.snapshot) == .none, "Activity showed it")
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
    viewer.send(.show(.activity))
    try await settle("Activity never read its log") { viewer.panes.activity.log?.entries.first?.id == "a3" }
    viewer.send(.show(.screen))
    viewer.send(.togglePause)
    try await Self.read(viewer, until: "a3")
    #expect(ViewerPresenter.unseen(viewer.snapshot) == .none, "Activity showed a2 and a3 before the status caught up")
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
