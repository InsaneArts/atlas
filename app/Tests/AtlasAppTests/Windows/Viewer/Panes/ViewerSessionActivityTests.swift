import AsyncSupportTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("Viewer Activity for one session")
struct ViewerSessionActivityTests {
  /// Two ended sessions the list names.
  static let first = "3f9a1c07b2e4"
  static let second = "8be20c4d1a77"

  @Test("choosing a session reads its own activity at once, and All reads every session's")
  func choosingASessionReadsItsOwnActivityAtOnce() async throws {
    let (service, viewer) = try await Self.watching()
    viewer.send(.show(.activity))
    try await settle("Activity was never read") { service.activityCalls == 1 }
    #expect(service.activitySessions.isEmpty, "every session's work is read while All shows")

    viewer.send(.chooseSession(.one(Self.first)))
    try await settle("the chosen session's activity was not read at once") {
      service.activitySessions == [Self.first]
    }
    try await settle("its log was not kept") { viewer.panes.activity.scope == .session(Self.first) }
    #expect(Self.streams(viewer) == [.session(Self.first)])

    let calls = service.activityCalls
    viewer.send(.chooseSession(.all))
    try await settle("every session's activity was not read at once") { service.activityCalls == calls + 1 }
    #expect(service.activitySessions == [Self.first])
    try await settle("every session's log was not kept") { viewer.panes.activity.scope == .all }
    viewer.close()
  }

  @Test("switching sessions shows what is held for the new one, never that nothing happened, until its log arrives")
  func switchingNeverFlashesTheEmptyMessage() async throws {
    let (service, viewer) = try await Self.watching()
    viewer.send(.chooseSession(.one(Self.first)))
    viewer.send(.show(.activity))
    try await settle("the first session's activity was never read") {
      viewer.panes.activity.scope == .session(Self.first)
    }

    service.hold.insert("activity")
    viewer.send(.chooseSession(.one(Self.second)))
    try await settle("the second session's activity was never asked for") {
      service.activitySessions.last == Self.second
    }
    let waiting = ViewerPresenter.activity(viewer.snapshot)
    #expect(waiting.empty == nil, "the first session's log says nothing of the second")
    #expect(waiting.days.isEmpty)
    #expect(!waiting.settled, "what shows is not the second session's own reading")

    service.resume("activity")
    try await settle("the second session's activity never arrived") {
      viewer.panes.activity.scope == .session(Self.second)
    }
    #expect(Self.streams(viewer) == [.session(Self.second)])
    let shown = ViewerPresenter.activity(viewer.snapshot)
    #expect(ViewerActivityFixtures.runs(shown).count == 1)
    #expect(shown.settled, "its own reading replaces what showed as one list")
    viewer.close()
  }

  @Test("a session whose steps were cleared says so, and one with none kept for another reason says nothing happened")
  func aClearedSessionSaysSo() async throws {
    let (service, viewer) = try await Self.watching()
    service.sessionLogs[Self.first] = ActivityLog(cleared: true)
    service.sessionLogs[Self.second] = ActivityLog()
    viewer.send(.chooseSession(.one(Self.first)))
    viewer.send(.show(.activity))
    try await settle("the first session's activity was never read") {
      viewer.panes.activity.scope == .session(Self.first)
    }
    let cleared = try #require(ViewerPresenter.activity(viewer.snapshot).empty)
    #expect(cleared.headline == ViewerWords.stepsCleared)
    #expect(cleared.message == ViewerWords.stepsClearedMessage)

    viewer.send(.chooseSession(.one(Self.second)))
    try await settle("the second session's activity was never read") {
      viewer.panes.activity.scope == .session(Self.second)
    }
    let none = try #require(ViewerPresenter.activity(viewer.snapshot).empty)
    #expect(none.headline == ViewerWords.noActivity)
    viewer.close()
  }

  /// The streams of the runs Activity shows.
  ///
  /// - Parameter viewer: The window.
  /// - Returns: Each run's stream, newest first.
  @MainActor
  private static func streams(_ viewer: ViewerController) -> [ActivityStream] {
    let moments = ViewerPresenter.activityMoments(viewer.snapshot)?.moments ?? []
    return moments.map(\.stream)
  }

  /// A window watching a service whose list names both sessions, each with one row in every session's log, once the
  /// list was read.
  ///
  /// - Returns: The service and the window.
  /// - Throws: The failed requirement, when the window never shows or the list is never read.
  @MainActor
  private static func watching() async throws -> (ScriptedService, ViewerController) {
    let service = ScriptedService()
    service.sessionList = HostSessionList(sessions: [
      HostSessionRecord(key: first, name: "Codex · demo", host: .codex, state: .ended, actions: 3),
      HostSessionRecord(key: second, name: "Pi · notes", host: .pi, state: .ended, actions: 1),
    ])
    service.activityLog = ActivityLog(entries: [
      ActivityLog.Entry(id: "second-row", says: "clicked Save", ageSeconds: 5, session: second, verb: "click"),
      ActivityLog.Entry(id: "first-row", says: "clicked Open", ageSeconds: 9, session: first, verb: "click"),
    ])
    let viewer = try await ViewerController.watching(service)
    try await settle("the list was never read") { viewer.panes.sessions.record(first) != nil }
    return (service, viewer)
  }
}
