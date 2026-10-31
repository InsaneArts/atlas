import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("PanelPresenter+Sessions")
struct PanelPresenterSessionsTests {
  @Test("one live session takes Recent's place, and a status that names no session keeps Recent")
  func oneSessionTakesRecentsPlace() throws {
    let one = PanelPresenter.state(try SessionReplies.snapshot([SessionReplies.claudeActing]))
    #expect(one.recent.isEmpty)
    #expect(one.sessions.map(\.name) == ["Claude Code · understudy"])
    #expect(one.sessions.map(\.line) == ["Clicking Show Center in Opie"])
    #expect(one.sessions.map(\.icon) == [.app(AgentLogos.claude)])
    #expect(one.moreSessions == nil)
    #expect(one.status.line == "Clicking Show Center in Opie · just now", "the line names no session")
    #expect(one.picture.caption?.icon == nil, "the caption keeps its blue dot")
    let older = PanelPresenter.state(
      PanelSnapshot(status: try status(["overview": ["mode": "agent", "state": "working"]]))
    )
    #expect(older.sessions.isEmpty)
    #expect(older.moreSessions == nil)
  }

  @Test("an ended session alone keeps Recent")
  func anEndedSessionAloneKeepsRecent() throws {
    var ended = SessionReplies.cursorIdle
    ended["state"] = "ended"
    ended["live"] = false
    let panel = PanelPresenter.state(try SessionReplies.snapshot([ended]))
    #expect(panel.sessions.isEmpty)
    #expect(panel.recent.map(\.line) == ["Clicking Show Center in Opie"])
  }

  @Test("one live session listed past its row still counts the rest")
  func oneListedSessionStillCountsTheRest() throws {
    let panel = PanelPresenter.state(try SessionReplies.snapshot([SessionReplies.claudeActing], more: 2))
    #expect(panel.sessions.count == 1)
    #expect(panel.moreSessions == "+2 more", "the two the service did not list")
  }

  @Test("Sessions take Recent's place with each session's line, state and icon, in the service's order")
  func sessionsTakeRecentsPlace() throws {
    let panel = PanelPresenter.state(
      try SessionReplies.snapshot([
        SessionReplies.claudeActing, SessionReplies.codexWaiting(), SessionReplies.cursorIdle,
      ])
    )
    #expect(panel.recent.isEmpty)
    #expect(panel.sessions.map(\.name) == ["Claude Code · understudy", "Codex · atlas-landing", "Cursor · opie"])
    #expect(
      panel.sessions.map(\.line) == [
        "Clicking Show Center in Opie", "Waiting for the helper · 8 s", "Last: opened Opie · 4 min ago",
      ]
    )
    #expect(panel.sessions.map(\.state) == ["Acting", "Waiting", "Idle"])
    #expect(panel.sessions.map(\.mark) == [.dot, .ring, .dot])
    #expect(panel.sessions.map(\.icon) == [.app(AgentLogos.claude), .app(AgentLogos.codex), .app(AgentLogos.cursor)])
    #expect(panel.picture.caption?.icon == .app(AgentLogos.claude), "the caption carries its session's icon")
    #expect(panel.sessions.first?.label == "Claude Code · understudy, Acting, Clicking Show Center in Opie")
  }

  @Test("the heading's line names who has the helper")
  func theLineNamesWhoHasTheHelper() throws {
    let panel = PanelPresenter.state(
      try SessionReplies.snapshot([SessionReplies.claudeActing, SessionReplies.cursorIdle])
    )
    #expect(panel.status.label == "Working")
    #expect(panel.status.line == "Claude Code · understudy has the helper")
  }

  @Test("the line names whose action may not have finished")
  func theLineNamesWhoseActionMayNotHaveFinished() throws {
    let panel = PanelPresenter.state(
      try SessionReplies.snapshot(
        [SessionReplies.claudeNeedsYou, SessionReplies.codexWaiting(recovery: true)],
        mode: "recovering",
        state: "needsYou",
        pending: 1
      )
    )
    #expect(panel.status.line == "A click by Claude Code may not have finished")
    #expect(panel.sessions.map(\.line) == ["A click may not have finished", "Waits until you recover"])
    #expect(panel.sessions.map(\.isUrgent) == [true, false])
    #expect(panel.sessions.first?.tone == .attention)
    #expect(panel.controls.main.event == .askRecover, "Recover… stays the only way out")
  }

  @Test("a notice still wins over the session line")
  func aNoticeStillWins() throws {
    var snapshot = try SessionReplies.snapshot([SessionReplies.claudeActing, SessionReplies.cursorIdle])
    snapshot.session.notice = "The agent could not be stopped. Try again."
    let panel = PanelPresenter.state(snapshot)
    #expect(panel.notice == "The agent could not be stopped. Try again.")
    #expect(panel.controls.agent == .stop, "Stop stays the one stop for every agent")
  }

  @Test("past three sessions the rest open from one row that counts them all")
  func pastThreeSessionsOneRowCountsTheRest() throws {
    let panel = PanelPresenter.state(
      try SessionReplies.snapshot(
        [
          SessionReplies.claudeActing, SessionReplies.codexWaiting(), SessionReplies.cursorIdle,
          SessionReplies.terminalIdle, SessionReplies.piIdle,
        ],
        more: 1
      )
    )
    #expect(panel.sessions.count == PanelState.mostSessions)
    #expect(panel.moreSessions == "+3 more", "the two not shown and the one the service did not list")
  }

  @Test("sessions hide while the helper is logged out or not ready, as Recent does")
  func sessionsHideWhileNotReady() throws {
    let panel = PanelPresenter.state(
      try SessionReplies.snapshot(
        [SessionReplies.claudeActing, SessionReplies.cursorIdle],
        mode: "loggedOut",
        state: "notReady"
      )
    )
    #expect(panel.sessions.isEmpty && panel.recent.isEmpty)
    #expect(panel.status.line == PanelWords.loggedOutLine)
  }

  @Test("an ended or quiet session's line says when, and Terminal's host is named")
  func endedAndQuietLinesSayWhen() {
    let now = Date(timeIntervalSince1970: 1_793_170_000)
    #expect(Words.sessionLine(.ended(at: now.addingTimeInterval(-1200)), now: now) == "Ended 20 min ago")
    #expect(Words.sessionLine(.lastSeen(at: now.addingTimeInterval(-7200)), now: now) == "Last seen 2 h ago")
    #expect(Words.sessionLine(.mayNotHaveFinished(verb: "type"), now: now) == "Typing may not have finished")
    #expect(Words.sessionLine(.waitingForHelper(seconds: nil), now: now) == "Waiting for the helper")
    #expect(Words.actionNoun("right-click") == "A click" && Words.actionNoun(nil) == "An action")
  }

  @Test("a row opens its session, All activity every session's, and the row for the rest the window's own choice")
  @MainActor
  func rowsOpenTheirSessions() {
    var opened: [ViewerSessionChoice?] = []
    let controller = PanelController.scripted(links: PanelLinks(openSessions: { opened.append($0) }))
    controller.send(.openSession("8be20c4d1a77"))
    controller.send(.openActivity)
    controller.send(.showAllSessions)
    #expect(opened == [.one("8be20c4d1a77"), .all, nil])
  }
  @Test("the panel retains an ended causer and hides an ordinary ended session")
  func thePanelRetainsAnEndedCauser() throws {
    var causer = SessionReplies.claudeNeedsYou
    causer["ended"] = 1000
    causer["live"] = false
    var ended = SessionReplies.cursorIdle
    ended["state"] = "ended"
    let snapshot = try SessionReplies.snapshot(
      [causer, ended, causer],
      mode: "recovering",
      state: "needsYou",
      pending: 1
    )
    let panel = PanelPresenter.state(snapshot)
    #expect(panel.sessions.map(\.id) == ["3f9a1c07b2e4"])
    #expect(panel.sessions.first?.isUrgent == true)
    #expect(panel.sessions.first?.line == "A click may not have finished")
    let recovered = PanelPresenter.state(try SessionReplies.snapshot([ended]))
    #expect(recovered.sessions.isEmpty)
  }

  @Test("the panel retains proven work whether or not the host is still live")
  func thePanelRetainsProvenWork() throws {
    for explicit in [false, true] {
      var work = SessionReplies.claudeActing
      work["workInProgress"] = true
      work["holdsHelper"] = false
      work["live"] = !explicit
      if explicit { work["ended"] = 1000 }
      let panel = PanelPresenter.state(try SessionReplies.snapshot([work]))
      #expect(panel.sessions.map(\.id) == ["3f9a1c07b2e4"])
      #expect(panel.sessions.first?.state == "Acting")
      work["state"] = "ended"
      work["live"] = false
      work["workInProgress"] = false
      #expect(PanelPresenter.state(try SessionReplies.snapshot([work])).sessions.isEmpty)
    }
  }

}
