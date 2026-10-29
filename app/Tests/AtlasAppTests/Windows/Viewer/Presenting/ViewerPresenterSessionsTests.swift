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

  @Test("the sidebar always lists the sessions, and only two or more live tell the holder apart in the header")
  func theSidebarAlwaysShowsAndTheHolderIsToldApartOnlyAmongTwo() throws {
    // A status that names no session lists All sessions alone; a single live session is listed, but not told apart.
    let none = ViewerPresenter.state(try .listing([]))
    #expect(none.sidebar.sessions.groups.isEmpty)
    #expect(none.sidebar.sessions.liveCount == 0, "All sessions says no count")
    #expect(none.sidebar.sessions.chosen == .all)
    var one = try ViewerSnapshot.listing([SessionReplies.claudeActing])
    #expect(ViewerPresenter.sessions(one).rows(.working).map(\.name) == ["Claude Code · understudy"])
    #expect(ViewerPresenter.header(one).what.isEmpty, "one session has the helper, but no one is told apart")
    one.session.chosenSession = .one("3f9a1c07b2e4")
    let chosen = ViewerPresenter.sessions(one)
    #expect(chosen.chosen == .one("3f9a1c07b2e4"))
    let three = try ViewerSnapshot.listing(Self.three)
    let column = ViewerPresenter.sessions(three)
    #expect(column.chosen == .all)
    #expect(column.icons.count == 3)
    #expect(ViewerPresenter.header(three) == ViewerHeaderState(who: "Claude Code · understudy", what: "has the helper"))
  }

  @Test("acting and waiting sessions are listed under Working, idle ones under Idle, each with its badge")
  func actingAndWaitingSessionsAreListedUnderWorking() throws {
    let sessions = ViewerPresenter.sessions(try .listing(Self.three))
    #expect(sessions.groups.map(\.section) == [.working, .idle])
    #expect(sessions.rows(.working).map(\.name) == ["Claude Code · understudy", "Codex · atlas-landing"])
    #expect(sessions.rows(.working).map(\.badge) == [.acting, .waiting])
    #expect(sessions.rows(.idle).map(\.name) == ["Cursor · opie"])
    #expect(sessions.rows(.idle).first?.badge == nil)
    #expect(sessions.rows(.working).first?.icon == SessionHost.claude.icon)
    #expect(sessions.liveCount == 3)
  }

  @Test("a row's line says what its session last did")
  func aRowsLineSaysWhatItLastDid() throws {
    let sessions = ViewerPresenter.sessions(try .listing(Self.three))
    let lines = sessions.groups
      .flatMap(\.rows)
      .map(\.line)
    #expect(lines == ["Clicking Show Center in Opie", "Waiting for the helper · 8 s", "Opened Opie · 4 min ago"])
  }

  @Test("Needs you is listed first and only while a session needs the person")
  func needsYouIsListedFirst() throws {
    let needsYou = try ViewerSnapshot.listing(
      [SessionReplies.codexWaiting(recovery: true), SessionReplies.claudeNeedsYou],
      mode: "recovering",
      state: "needsYou",
      pending: 1
    )
    let sessions = ViewerPresenter.sessions(needsYou)
    #expect(sessions.groups.first?.section == .needsYou)
    #expect(sessions.rows(.needsYou).map(\.name) == ["Claude Code · understudy"])
    #expect(sessions.rows(.needsYou).first?.badge == .needsYou)
    #expect(sessions.rows(.needsYou).first?.line == "A click may not have finished")
    let calm = ViewerPresenter.sessions(try .listing(Self.three))
    #expect(!calm.groups.contains { $0.section == .needsYou })
  }

  @Test("ended sessions fold under Ended with their count, and open to list each")
  func endedSessionsFoldUnderEnded() throws {
    var snapshot = try ViewerSnapshot.listing(Self.three)
    snapshot.panes.sessions.list = HostSessionList(sessions: [
      HostSessionRecord(key: "c0ffee5e5510", name: "Pi · notes-sync", host: .pi, state: .ended),
      HostSessionRecord(key: "aaaa00000001", name: "Claude Code · opie", host: .claude, state: .ended),
    ])
    let folded = ViewerPresenter.sessions(snapshot)
    #expect(folded.groups.last?.section == .ended)
    #expect(folded.endedCount == 2)
    #expect(folded.rows(.ended).isEmpty)
    #expect(!folded.endedShown)
    snapshot.session.endedShown = true
    let open = ViewerPresenter.sessions(snapshot)
    #expect(open.rows(.ended).map(\.name) == ["Pi · notes-sync", "Claude Code · opie"])
    #expect(open.rows(.ended).allSatisfy { $0.badge == nil })
  }

  @Test("a chosen ended session stays listed while Ended is folded")
  func aChosenEndedSessionStaysListed() throws {
    var snapshot = try ViewerSnapshot.listing(Self.three)
    snapshot.panes.sessions.list = HostSessionList(sessions: [
      HostSessionRecord(key: "c0ffee5e5510", name: "Pi · notes-sync", host: .pi, state: .ended),
      HostSessionRecord(key: "aaaa00000001", name: "Claude Code · opie", host: .claude, state: .ended),
    ])
    snapshot.session.chosenSession = .one("c0ffee5e5510")
    let sessions = ViewerPresenter.sessions(snapshot)
    #expect(sessions.rows(.ended).map(\.id) == ["c0ffee5e5510"])
    #expect(sessions.endedCount == 2)
    #expect(sessions.choices.last == .one("c0ffee5e5510"), "↓ reaches it where it is drawn")
  }

  @Test("the search keeps the rows whose name or line holds the query and folds empty sections")
  func theSearchKeepsTheRowsThatHoldTheQuery() throws {
    var snapshot = try ViewerSnapshot.listing(Self.three)
    snapshot.session.search = ViewerSearchState(isOpen: true, query: "OPIE")
    let sessions = ViewerPresenter.sessions(snapshot)
    #expect(sessions.groups.map(\.section) == [.working, .idle])
    #expect(sessions.rows(.working).map(\.name) == ["Claude Code · understudy"], "its line names Opie")
    #expect(sessions.rows(.idle).map(\.name) == ["Cursor · opie"])
    snapshot.session.search.query = "cursor"
    #expect(ViewerPresenter.sessions(snapshot).groups.map(\.section) == [.idle], "Working holds no match, so it folds")
    #expect(sessions.liveCount == 3, "All sessions still counts every live session")
    snapshot.session.search.query = "waiting"
    #expect(ViewerPresenter.sessions(snapshot).rows(.working).map(\.name) == ["Codex · atlas-landing"])
    snapshot.session.search.query = " "
    #expect(ViewerPresenter.sessions(snapshot).groups.count == 2, "a blank query keeps every row")
  }

  @Test("the column lists every live session however many there are")
  func theColumnListsEveryLiveSession() throws {
    let many = (0..<12)
      .map { hostSession(String(format: "%012x", $0), host: "codex", name: "Codex · project \($0)") }
    let column = ViewerPresenter.sessions(try .listing(many))
    #expect(column.groups.flatMap(\.rows).count == 12)
    #expect(column.icons.count == 3, "All sessions stacks three icons")
  }

  @Test("the column lists the status's eight, then the list's other live sessions, first commands included, then Ended")
  func theColumnJoinsTheStatusAndTheList() throws {
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
    snapshot.session.endedShown = true
    let column = ViewerPresenter.sessions(snapshot)
    let live = (column.rows(.working) + column.rows(.idle))
      .map(\.id)
    #expect(Set(live) == Set(statusOnes.map(\.key) + ["aaaa00000001", "aaaa00000002", "aaaa00000003"]))
    #expect(column.rows(.working).map(\.id) == ["aaaa00000002"], "the acting one the list alone names")
    #expect(column.rows(.idle).suffix(2).map(\.id) == ["aaaa00000001", "aaaa00000003"], "after the status's, in order")
    let ended = column.rows(.ended)
      .map(\.id)
    #expect(ended == ["c0ffee5e5510"])
    #expect(column.rows(.idle).last?.icon == SessionHost.pi.icon, "a first command's session has its host's icon")
    #expect(ViewerPresenter.resolvedSession("aaaa00000003", snapshot).name == "Pi · first")
  }

  @Test("a session only the list names live reads its own state in the sidebar, as its Activity header does")
  func aSessionOnlyTheListNamesLiveReadsItsOwnState() throws {
    var snapshot = try ViewerSnapshot.listing(Self.three)
    let cursor = ViewerStatus.HostSession(key: "aaaa00000002", name: "Cursor · notes", host: .cursor, state: .acting)
    snapshot.panes.sessions.list = HostSessionList(
      sessions: [HostSessionRecord(key: "aaaa00000002", name: "Cursor · notes", host: .cursor)],
      live: [cursor]
    )
    snapshot.session.chosenSession = .one("aaaa00000002")
    let resolved = ViewerPresenter.resolvedSession("aaaa00000002", snapshot)
    #expect(resolved.state == .acting && resolved.live == cursor && resolved.listed == nil)
    let row = try ViewerPresenterSessionMenuTests.row("aaaa00000002", snapshot)
    #expect(row.badge == .acting, "not Idle, as the status no longer lists it")
    #expect(ViewerPresenter.sessions(snapshot).rows(.working).map(\.id).contains("aaaa00000002"))
    let quiet = ViewerPresenter.resolvedSession("c0ffee5e5510", snapshot)
    #expect(quiet.state == nil && quiet.name == "Another session", "a session nothing names has no state")
  }

  @Test("a session nothing names has no icon, and only Terminal has the prompt mark")
  func anUnknownSessionHasNoIcon() throws {
    var snapshot = try ViewerSnapshot.listing(Self.three)
    snapshot.panes.sessions.list = HostSessionList()
    #expect(ViewerPresenter.resolvedSession("dead00000000", snapshot).icon == nil)
    #expect(ViewerPresenter.resolvedSession("", snapshot).icon == SessionHost.terminal.icon)
    let reading = snapshot.reading.statusReading
    #expect(SessionIcons.icon(of: "dead00000000", in: reading) == nil)
    #expect(SessionIcons.icon(of: "", in: reading) == SessionHost.terminal.icon)
    #expect(SessionIcons.icon(of: "8be20c4d1a77", in: reading) == SessionHost.codex.icon)
  }

  @Test("the footer's latest line is the newest action with its session's icon")
  func theFootersLatestLineIsTheNewestActionWithItsIcon() throws {
    let codexShot: [String: Any] = [
      "operation": "screenshot", "says": "took a screenshot", "ok": true, "ageSeconds": 0, "session": "8be20c4d1a77",
    ]
    let other = ViewerPresenter.latest(try .listing(Self.three, activity: [codexShot]))
    #expect(other.words == "Took a screenshot")
    #expect(other.icon == SessionHost.codex.icon, "Codex's screenshot, not the holder's")
    #expect(other.age == "just now")
    let claudeClick: [String: Any] = [
      "operation": "input", "says": "clicking Show Center in Opie", "ok": true, "started": true, "ageSeconds": 0,
      "session": "3f9a1c07b2e4",
    ]
    var own = try ViewerSnapshot.listing(Self.three, activity: [claudeClick])
    #expect(ViewerPresenter.latest(own).icon == SessionHost.claude.icon)
    #expect(ViewerPresenter.latest(own).words == "Clicking Show Center in Opie", "the header names the holder")
    own.session.place = .activity
    #expect(ViewerPresenter.latest(own).words == "Clicking Show Center in Opie", "every place shows the same line")
  }

  @Test("in All sessions each moment names its session, and the person's own say You")
  func inAllSessionsEachMomentNamesItsSession() throws {
    var snapshot = try ViewerSnapshot.listing([
      SessionReplies.claudeActing, SessionReplies.terminalIdle, SessionReplies.cursorIdle,
    ])
    snapshot.session.place = .activity
    let rows = [
      ActivityLog.Entry(id: "t1", says: "ran a command", ageSeconds: 0, operation: "run"),
      ActivityLog.Entry(
        id: "c1",
        says: "opened Opie",
        ageSeconds: 70,
        operation: "open",
        session: "5d21e9a04c3f",
        app: "Opie"
      ),
      ActivityLog.Entry(id: "y1", says: "clicked in Opie", actor: .you, ageSeconds: 80, app: "Opie"),
      ActivityLog.Entry(id: "a1", says: "clicked Save", ageSeconds: 90, session: "3f9a1c07b2e4", app: "TextEdit"),
    ]
    snapshot.panes.activity.log = ActivityLog(entries: rows)
    snapshot.panes.activity.readAt = snapshot.date
    let moments = ViewerPresenter.activity(snapshot)
      .items
      .compactMap { item -> ViewerActivityMoment? in
        if case .moment(let moment) = item { return moment }
        return nil
      }
    #expect(moments.map(\.session?.name) == ["Terminal", "Cursor · opie", "You", "Claude Code · understudy"])
    #expect(moments.map(\.title) == ["Commands", "Opie", "Opie", "TextEdit"])
    #expect(moments[2].session?.icon == nil, "the person's own moment leads with their mark")
  }

  @Test("a copy and a plan say what is known, with no empty value where a time would be")
  func copiesAndPlansSayWhatIsKnown() throws {
    var snapshot = try ViewerSnapshot.listing(Self.three)
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
    let record = try #require(snapshot.panes.sessions.record("3f9a1c07b2e4"))
    #expect(
      ViewerPresenter.sessionCopies(record) == [
        Fact(name: "Copy of Opie", value: ""), Fact(name: "Copies of Notes", value: "2"),
      ]
    )
    let plans = ViewerPresenter.sessionPlans(record, snapshot)
    #expect(plans.map(\.name) == ["Environment plan", "Environment plan"])
    #expect(plans.first?.value.hasPrefix("Applied · can roll back · made at ") == true)
    #expect(plans.last?.value == "A plan", "no empty time where none is known")
    #expect(ViewerPresenter.sessionShots(record) == [Fact(name: "Screenshots", value: "1")])
  }

  @Test("a chosen session that is gone falls back to all")
  func aChosenSessionThatIsGoneFallsBackToAll() throws {
    var snapshot = try ViewerSnapshot.listing(Self.three)
    snapshot.session.chosenSession = .one("c0ffee5e5510")
    #expect(snapshot.shownSessions == .all, "neither the status nor the list names it")
    snapshot.panes.sessions.list = HostSessionList(sessions: [
      HostSessionRecord(key: "c0ffee5e5510", name: "Pi · notes-sync", host: .pi, state: .ended)
    ])
    #expect(snapshot.shownSessions == .one("c0ffee5e5510"), "the list still names it as ended")
    var unread = ViewerSnapshot()
    unread.session.chosenSession = .one("3f9a1c07b2e4")
    #expect(unread.shownSessions == .one("3f9a1c07b2e4"), "before the first reading the choice holds")
  }

  @Test("the header names the session that has the helper, and gives way to pause and control")
  func theHeaderNamesTheHolder() throws {
    var snapshot = try ViewerSnapshot.listing(Self.three)
    #expect(
      ViewerPresenter.header(snapshot) == ViewerHeaderState(who: "Claude Code · understudy", what: "has the helper")
    )
    snapshot.session.paused = true
    #expect(ViewerPresenter.header(snapshot).who == "Paused")
    #expect(ViewerPresenter.header(snapshot).what.hasPrefix("picture from "), "the paused picture's time")
    snapshot.lease = "lease"
    #expect(ViewerPresenter.header(snapshot) == ViewerHeaderState(who: "You", what: "have control of the helper"))
    snapshot.composing = "こん"
    #expect(ViewerPresenter.latest(snapshot).words == "Typing こん", "what is typed takes the footer's line")
    #expect(ViewerPresenter.latest(snapshot).caret)
  }

  @Test("the helper's line says whose action may not have finished")
  func theHelpersLineNamesTheCauser() throws {
    let needsYou = try ViewerSnapshot.listing(
      [SessionReplies.claudeNeedsYou, SessionReplies.codexWaiting(recovery: true)],
      mode: "recovering",
      state: "needsYou",
      pending: 1
    )
    #expect(ViewerPresenter.status(needsYou).line == "A click by Claude Code may not have finished")
  }

  @Test("an older service with no list leaves the sidebar without Ended and each session with the status's facts")
  func noListLeavesNoEndedAndTheStatussFacts() throws {
    var snapshot = try ViewerSnapshot.listing(Self.three)
    snapshot.session.chosenSession = .one("3f9a1c07b2e4")
    snapshot.panes.sessions.notOfferedAt = snapshot.date
    #expect(ViewerPresenter.sessions(snapshot).endedCount == 0)
    let row = try ViewerPresenterSessionMenuTests.row("3f9a1c07b2e4", snapshot)
    #expect(row.help == "Claude Code · understudy\nClicking Show Center in Opie\nHost: Claude Code")
    #expect(!row.menu.flatMap(\.items).contains { $0.role == .destructive }, "nothing to clean up is known")
  }
}
