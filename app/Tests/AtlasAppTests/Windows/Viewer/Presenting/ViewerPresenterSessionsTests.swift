import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("ViewerPresenter+Sessions")
struct ViewerPresenterSessionsTests {
  /// Claude Code with the helper, Codex waiting its turn and Cursor idle.
  static var three: [[String: Any]] {
    [SessionReplies.claudeActing, SessionReplies.codexWaiting(), SessionReplies.cursorIdle]
  }

  @Test("the menu button shows only while two sessions are live or one is chosen")
  func theButtonShowsOnlyWithTwoLiveOrOneChosen() throws {
    // A status that names no session, and one with a single live session, read as today: no button, no session words.
    #expect(ViewerPresenter.top(try .listing([])).sessions == nil)
    var one = try ViewerSnapshot.listing([SessionReplies.claudeActing])
    #expect(ViewerPresenter.top(one).sessions == nil)
    #expect(ViewerPresenter.screen(one).chip == nil, "one session has the helper, but no one is told apart")
    #expect(ViewerPresenter.screen(one).captions.allSatisfy { $0.icon == nil })
    one.session.sessionChoice = .one("3f9a1c07b2e4")
    #expect(ViewerPresenter.top(one).sessions?.title == "Claude Code · understudy", "a chosen session keeps it")
    let button = try #require(ViewerPresenter.top(try .listing(Self.three)).sessions)
    #expect(button.title == "All sessions")
    #expect(button.icons.count == 3)
  }

  @Test("the menu lists every live session however many there are")
  func theMenuListsEveryLiveSession() throws {
    let many = (0..<12)
      .map { hostSession(String(format: "%012x", $0), host: "codex", name: "Codex · project \($0)") }
    let menu = ViewerPresenter.sessionsMenu(try .listing(many))
    #expect(menu.live.count == 12)
    #expect(ViewerPresenter.top(try .listing(many)).sessions?.icons.count == 3, "the button stacks three icons")
  }

  @Test("the menu lists the status's eight, then the list's other live sessions, first commands included, then Ended")
  func theMenuJoinsTheStatusAndTheList() throws {
    let listed = (0..<8)
      .map { hostSession(String(format: "%012x", $0), host: "codex", name: "Codex · project \($0)") }
    var snapshot = try ViewerSnapshot.listing(listed, more: 3)
    // The list's live sessions are in the status's order: the eight the status lists, then the three it had no room
    // for, one of them on its first command, with no record yet.
    let statusOnes = snapshot.reading.statusReading.liveSessions
    let past = [
      ViewerStatus.HostSession(key: "aaaa00000001", name: "Claude Code · opie", host: .claude),
      ViewerStatus.HostSession(key: "aaaa00000002", name: "Cursor · notes", host: .cursor, state: .acting),
      ViewerStatus.HostSession(key: "aaaa00000003", name: "Pi · first", host: .pi),
    ]
    snapshot.panes.sessions.list = HostSessionList(
      sessions: [
        HostSessionRecord(key: "aaaa00000001", name: "Claude Code · opie", host: .claude),
        HostSessionRecord(key: "c0ffee5e5510", name: "Pi · notes-sync", host: .pi, state: .ended),
      ],
      live: statusOnes + past
    )
    let menu = ViewerPresenter.sessionsMenu(snapshot)
    let live = menu.live.map(\.key)
    #expect(live == statusOnes.map(\.key) + ["aaaa00000001", "aaaa00000002", "aaaa00000003"])
    let ended = menu.ended.map(\.key)
    #expect(ended == ["c0ffee5e5510"])
    #expect(menu.live.last?.icon == SessionIcons.icon(.pi), "a first command's session has its host's icon")
    #expect(ViewerPresenter.sessionName("aaaa00000003", snapshot) == "Pi · first")
  }

  @Test("a session nothing names has no icon, and only Terminal has the >_ tile")
  func anUnknownSessionHasNoIcon() throws {
    var snapshot = try ViewerSnapshot.listing(Self.three)
    snapshot.panes.sessions.list = HostSessionList()
    #expect(ViewerPresenter.sessionIcon("dead00000000", snapshot) == nil)
    #expect(ViewerPresenter.sessionIcon("", snapshot) == SessionIcons.icon(.terminal))
    let reading = snapshot.reading.statusReading
    #expect(SessionIcons.icon(of: "dead00000000", in: reading) == nil)
    #expect(SessionIcons.icon(of: "", in: reading) == SessionIcons.icon(.terminal))
    #expect(SessionIcons.icon(of: "8be20c4d1a77", in: reading) == SessionIcons.icon(.codex))
  }

  @Test("the holder leads the bar's line only for its own action, with every session shown and no chip naming it")
  func theHolderLeadsOnlyItsOwnAction() throws {
    let codexShot: [String: Any] = [
      "operation": "screenshot", "says": "took a screenshot", "ok": true, "ageSeconds": 0, "session": "8be20c4d1a77",
    ]
    var other = try ViewerSnapshot.listing(Self.three, activity: [codexShot])
    other.session.tab = .activity
    #expect(ViewerPresenter.status(other).line == "Took a screenshot", "Codex's screenshot is not Claude Code's")
    let claudeClick: [String: Any] = [
      "operation": "input", "says": "clicking Show Center in Opie", "ok": true, "started": true, "ageSeconds": 0,
      "session": "3f9a1c07b2e4",
    ]
    var own = try ViewerSnapshot.listing(Self.three, activity: [claudeClick])
    own.session.tab = .activity
    #expect(ViewerPresenter.status(own).line == "Claude Code · Clicking Show Center in Opie")
    var twoClaudes = try ViewerSnapshot.listing(
      Self.three + [hostSession("aaaa00000001", host: "claude", name: "Claude Code · opie")],
      activity: [claudeClick]
    )
    twoClaudes.session.tab = .activity
    #expect(
      ViewerPresenter.status(twoClaudes).line == "Claude Code · understudy · Clicking Show Center in Opie",
      "with two sessions in one host, the full name tells them apart"
    )
    own.session.sessionChoice = .one("8be20c4d1a77")
    #expect(ViewerPresenter.status(own).line == "Clicking Show Center in Opie", "a chosen session drops the name")
    own.session.sessionChoice = .all
    own.session.tab = .screen
    #expect(ViewerPresenter.holderLeads(own) == nil, "the Screen's chip names the holder already")
  }

  @Test("Terminal's group keeps the status's place and its state word, and Show all names its session")
  func terminalsGroupKeepsItsPlace() throws {
    var snapshot = try ViewerSnapshot.listing([
      SessionReplies.claudeActing, SessionReplies.terminalIdle, SessionReplies.cursorIdle,
    ])
    snapshot.session.tab = .activity
    let terminal = (0..<6)
      .map { ActivityLog.Entry(id: "t\($0)", says: "ran command \($0)", ageSeconds: Double($0 * 10)) }
    let rows =
      terminal + [
        ActivityLog.Entry(id: "c1", says: "opened Opie", ageSeconds: 70, session: "5d21e9a04c3f"),
        ActivityLog.Entry(id: "y1", says: "clicked in Opie", actor: .you, ageSeconds: 80),
        ActivityLog.Entry(id: "a1", says: "clicked Save", ageSeconds: 90, session: "3f9a1c07b2e4"),
      ]
    snapshot.panes.activity.log = ActivityLog(entries: rows)
    snapshot.panes.activity.readAt = snapshot.date
    let activity = ViewerPresenter.activity(snapshot)
    let names = activity.sessions.map(\.name)
    #expect(names == ["Claude Code · understudy", "Terminal", "Cursor · opie", "You"])
    let group = try #require(activity.sessions.first { $0.name == "Terminal" })
    #expect(group.state == "Idle")
    #expect(group.moreLabel == "Show all 6 from Terminal")
  }

  @Test("a copy and a plan say what is known, with no empty value where a time would be")
  func copiesAndPlansSayWhatIsKnown() throws {
    var snapshot = try ViewerSnapshot.listing(Self.three)
    snapshot.session.sessionChoice = .one("3f9a1c07b2e4")
    snapshot.detailsShown = true
    snapshot.panes.sessions.list = HostSessionList(sessions: [
      HostSessionRecord(
        key: "3f9a1c07b2e4",
        name: "Claude Code · understudy",
        host: .claude,
        copies: [HostSessionCopy(app: "Opie"), HostSessionCopy(app: "Notes"), HostSessionCopy(app: "Notes")],
        plans: [
          HostSessionPlan(phase: .applied, madeAt: snapshot.date, canRollBack: true),
          HostSessionPlan(phase: .unknown),
        ],
        screenshots: ["1793163300000000000-capture.png"]
      )
    ])
    let detail = try #require(ViewerPresenter.details(snapshot).hostSession)
    #expect(detail.copies == [Fact(name: "Copy of Opie", value: ""), Fact(name: "Copies of Notes", value: "2")])
    let plans = detail.plans.map(\.name)
    #expect(plans == ["Applied · can roll back", "A plan"])
    #expect(detail.plans.first?.value.hasPrefix("Made at ") == true)
    #expect(detail.plans.last?.value == "")
    #expect(detail.shots.first?.label.hasPrefix("Screenshot taken at ") == true)
  }

  @Test("a chosen session that is gone falls back to all")
  func aChosenSessionThatIsGoneFallsBackToAll() throws {
    var snapshot = try ViewerSnapshot.listing(Self.three)
    snapshot.session.sessionChoice = .one("c0ffee5e5510")
    #expect(snapshot.sessionChoice == .all, "neither the status nor the list names it")
    snapshot.panes.sessions.list = HostSessionList(sessions: [
      HostSessionRecord(key: "c0ffee5e5510", name: "Pi · notes-sync", host: .pi, state: .ended)
    ])
    #expect(snapshot.sessionChoice == .one("c0ffee5e5510"), "the list still names it as ended")
    var unread = ViewerSnapshot()
    unread.session.sessionChoice = .one("3f9a1c07b2e4")
    #expect(unread.sessionChoice == .one("3f9a1c07b2e4"), "before the first reading the choice holds")
  }

  @Test("the holder chip gives way to typing, control and pause")
  func theHolderChipGivesWay() throws {
    var snapshot = try ViewerSnapshot.listing(Self.three)
    #expect(
      ViewerPresenter.chip(snapshot) == .holder(name: "Claude Code · understudy", icon: SessionIcons.icon(.claude))
    )
    snapshot.session.paused = true
    #expect(ViewerPresenter.chip(snapshot) == .paused)
    snapshot.lease = "lease"
    #expect(ViewerPresenter.chip(snapshot) == .inControl)
    snapshot.composing = "こん"
    #expect(ViewerPresenter.chip(snapshot) == .typing("こん"))
  }

  @Test("the bar's line names who has the helper, and whose action may not have finished")
  func theBarsLineNamesTheHolderAndTheCauser() throws {
    var working = try ViewerSnapshot.listing(
      Self.three,
      activity: [
        [
          "operation": "input", "says": "clicking Show Center in Opie", "ok": true, "started": true, "ageSeconds": 0,
          "session": "3f9a1c07b2e4",
        ]
      ]
    )
    working.session.tab = .activity
    #expect(ViewerPresenter.status(working).line == "Claude Code · Clicking Show Center in Opie")
    let needsYou = try ViewerSnapshot.listing(
      [SessionReplies.claudeNeedsYou, SessionReplies.codexWaiting(recovery: true)],
      mode: "recovering",
      state: "needsYou",
      pending: 1
    )
    #expect(ViewerPresenter.status(needsYou).line == "A click by Claude Code may not have finished")
  }

  @Test("an older service with no list leaves the menu without Ended and Details without the session")
  func noListLeavesNoEndedAndNoDetail() throws {
    var snapshot = try ViewerSnapshot.listing(Self.three)
    snapshot.session.sessionChoice = .one("3f9a1c07b2e4")
    snapshot.detailsShown = true
    snapshot.panes.sessions.notOfferedAt = snapshot.date
    #expect(ViewerPresenter.sessionsMenu(snapshot).ended.isEmpty)
    #expect(ViewerPresenter.details(snapshot).hostSession == nil)
  }
}
