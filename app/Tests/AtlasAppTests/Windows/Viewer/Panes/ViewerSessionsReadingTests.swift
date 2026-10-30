import AsyncSupportTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("ViewerPanesModel+Sessions")
struct ViewerSessionsReadingTests {
  @Test("history refreshes with status readings without keeping a polling clock wait")
  func historyRefreshesWithStatusReadings() async throws {
    let service = ScriptedService()
    let clock = ManualClock()
    let now = Date(timeIntervalSince1970: 1_793_170_000)
    let viewer = try await ViewerController.watching(service, clock: clock, wallClock: { now })
    defer { viewer.close() }
    try await settle("history discovery never ended") {
      viewer.panes.sessions.list != nil && viewer.panesModel.work.sessions == nil
    }
    let asked = service.sessionsCalls
    await viewer.readingLoop.readStatus()
    #expect(service.sessionsCalls == asked)
    viewer.readingLoop.watch(false)
    #expect(clock.waiting.isEmpty)
    clock.advance(by: .seconds(10))
    await viewer.readingLoop.readStatus()
    try await settle("status never refreshed history") { service.sessionsCalls == asked + 1 }
    await viewer.panesModel.work.sessions?.value
    #expect(viewer.panesModel.work.sessions == nil)
    #expect(clock.waiting.isEmpty)
  }

  @Test("moving the system clock back never delays a session refresh")
  func movingTheSystemClockBackNeverDelaysARefresh() async throws {
    let service = ScriptedService()
    let clock = ManualClock()
    var date = Date(timeIntervalSince1970: 1_793_170_000)
    let viewer = try await ViewerController.watching(service, clock: clock, wallClock: { date })
    defer { viewer.close() }
    try await settle("history discovery never ended") {
      viewer.panes.sessions.list != nil && viewer.panesModel.work.sessions == nil
    }
    viewer.readingLoop.watch(false)
    let asked = service.sessionsCalls
    date = date.addingTimeInterval(-3_600)
    clock.advance(by: .seconds(9))
    await viewer.readingLoop.readStatus()
    #expect(service.sessionsCalls == asked, "history still respects its refresh interval")
    clock.advance(by: .seconds(1))
    await viewer.readingLoop.readStatus()
    try await settle("moving the date back stalled history") { service.sessionsCalls == asked + 1 }
  }

  @Test("moving the system clock back never delays an unsupported service retry")
  func movingTheSystemClockBackNeverDelaysARetry() async throws {
    let service = ScriptedService()
    service.offersSessions = false
    let clock = ManualClock()
    var date = Date(timeIntervalSince1970: 1_793_170_000)
    let viewer = try await ViewerController.watching(service, clock: clock, wallClock: { date })
    defer { viewer.close() }
    try await settle("the unsupported reply never arrived") { viewer.panes.sessions.notOfferedAt != nil }
    viewer.readingLoop.watch(false)
    date = date.addingTimeInterval(-3_600)
    clock.advance(by: .seconds(ViewerSessionsFacts.askAgainAfter - 1))
    await viewer.readingLoop.readStatus()
    #expect(service.sessionsCalls == 1)
    service.offersSessions = true
    clock.advance(by: .seconds(1))
    await viewer.readingLoop.readStatus()
    try await settle("moving the date back stalled the retry") { viewer.panes.sessions.list != nil }
    #expect(service.sessionsCalls == 2)
    #expect(viewer.panes.sessions.notOfferedUptime == nil)
  }

  /// A status reply listing Claude Code with the helper and Codex waiting, as two live sessions.
  static var twoLive: [String: Any] {
    [
      "ready": true, "canObserve": true,
      "overview": [
        "mode": "agent", "state": "working", "sessionsShown": true, "holder": "3f9a1c07b2e4",
        "hostSessions": [SessionReplies.claudeActing, SessionReplies.codexWaiting()],
      ],
    ]
  }

  @Test("a fresh visible window reads ended sessions with zero or one live session", arguments: [false, true])
  func aFreshWindowReadsEndedSessions(oneLive: Bool) async throws {
    let service = ScriptedService()
    if oneLive {
      service.reply = [
        "ready": true, "canObserve": true,
        "overview": ["mode": "agent", "state": "working", "hostSessions": [SessionReplies.claudeActing]],
      ]
    }
    service.sessionList = HostSessionList(sessions: [
      HostSessionRecord(key: "c0ffee5e5510", name: "Pi · notes-sync", host: .pi, state: .ended)
    ])
    let viewer = try await ViewerController.watching(service)
    try await settle("the list was never read") { viewer.panes.sessions.list != nil }
    viewer.send(.toggleEnded)
    let ended = ViewerPresenter.sessions(viewer.snapshot)
      .rows(.ended)
      .map(\.name)
    #expect(ended == ["Pi · notes-sync"])
    viewer.close()
    #expect(viewer.panesModel.work.sessions == nil, "a closed window stops reading it")
  }

  @Test("a service that offers no list gives no Ended part and is asked again a minute later")
  func aServiceWithNoListIsAskedAgainLater() async throws {
    let service = ScriptedService()
    service.offersSessions = false
    service.reply = Self.twoLive
    var now = Date(timeIntervalSince1970: 1_793_170_000)
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(service, clock: clock, wallClock: { now })
    try await settle("the list was never asked for") { viewer.panes.sessions.notOfferedAt != nil }
    #expect(service.sessionsCalls == 1)
    viewer.send(.chooseSession(.one("3f9a1c07b2e4")))
    await viewer.readingLoop.readStatus()
    #expect(service.sessionsCalls == 1, "it is not asked again within the minute")
    #expect(ViewerPresenter.sessions(viewer.snapshot).endedCount == 0)
    #expect(viewer.panesModel.work.sessions == nil)
    // Updated meanwhile, the service offers the list, and the next status a minute on asks for it.
    service.offersSessions = true
    service.sessionList = HostSessionList(sessions: [
      HostSessionRecord(key: "c0ffee5e5510", name: "Pi · notes-sync", host: .pi, state: .ended)
    ])
    clock.advance(by: .seconds(ViewerSessionsFacts.askAgainAfter))
    await viewer.readingLoop.readStatus()
    try await settle("the list was never asked for again") { viewer.panes.sessions.list != nil }
    #expect(service.sessionsCalls == 2)
    #expect(viewer.panes.sessions.notOfferedAt == nil)
  }

  @Test("a session chosen in the column holds, and All sessions shows every session's work again")
  func aChosenSessionHolds() async throws {
    let service = ScriptedService()
    service.reply = Self.twoLive
    let viewer = try await ViewerController.watching(service)
    viewer.send(.chooseSession(.one("8be20c4d1a77")))
    #expect(viewer.snapshot.shownSessions == .one("8be20c4d1a77"))
    viewer.send(.chooseSession(.all))
    #expect(viewer.snapshot.shownSessions == .all)
  }
}
