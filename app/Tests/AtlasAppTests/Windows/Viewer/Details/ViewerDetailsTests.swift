import AtlasService
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("ViewerPresenter+Details")
struct ViewerDetailsTests {
  @Test("the session leads Details and the helper's login reads Login")
  func theSessionLeadsDetails() throws {
    var snapshot = try ViewerSnapshot.listing(ViewerPresenterSessionsTests.three)
    snapshot.detailsShown = true
    snapshot.agents = [.claude]
    snapshot.timeZone = TimeZone(secondsFromGMT: 0) ?? .current
    snapshot.locale = Locale(identifier: "de_DE")
    snapshot.panes.sessions.list = HostSessionList(sessions: [
      HostSessionRecord(
        key: "3f9a1c07b2e4",
        name: "Claude Code · understudy",
        host: .claude,
        hostLabel: "Claude Code",
        folder: "understudy",
        startedAt: snapshot.date.addingTimeInterval(-3600),
        lastSeenAt: snapshot.date,
        apps: [HostSessionApp(name: "Opie", copied: true), HostSessionApp(name: "Notes")],
        copies: [HostSessionCopy(app: "Opie"), HostSessionCopy(app: "Opie")],
        plans: [HostSessionPlan(phase: .applied, canRollBack: true), HostSessionPlan(phase: .rollingBack)],
        screenshots: ["a.png", "b.png", "c.png", "d.png"]
      )
    ])
    #expect(ViewerPresenter.details(snapshot).hostSession == nil, "every session's work shows")
    snapshot.session.sessionChoice = .one("3f9a1c07b2e4")
    let details = ViewerPresenter.details(snapshot)
    let session = try #require(details.hostSession)
    #expect(session.state == "Acting · has the helper")
    #expect(session.facts.map(\.name) == ["Host", "Folder", "Started", "Last seen"])
    #expect(session.facts.first?.value == "Claude Code")
    // Before the Apps tab has read anything, an app reads by its name alone.
    #expect(session.apps.map(\.value) == ["", ""])
    #expect(session.copies == [Fact(name: "Copies of Opie", value: "2")])
    let plans = session.plans.map(\.name)
    #expect(plans == ["Applied · can roll back", "Rolling back"])
    #expect(session.shots.map(\.name) == ["a.png", "b.png", "c.png"])
    #expect(session.allShots == "See all 4")
    #expect(details.login.map(\.name) == ["Agents"], "the helper's login keeps its rows, under Login")
    snapshot.panes.apps.apps = [HelperApp(pid: 1, name: "Opie")]
    #expect(ViewerPresenter.details(snapshot).hostSession?.apps.map(\.value) == ["Open · a copy", "Closed"])
  }

  @Test("Terminal's host is none named")
  func terminalsHostIsNoneNamed() throws {
    var snapshot = try ViewerSnapshot.listing([SessionReplies.claudeActing, SessionReplies.terminalIdle])
    snapshot.detailsShown = true
    snapshot.session.sessionChoice = .one("")
    snapshot.panes.sessions.list = HostSessionList(sessions: [
      HostSessionRecord(key: "", name: "Terminal", host: .terminal)
    ])
    #expect(ViewerPresenter.details(snapshot).hostSession?.facts.first == Fact(name: "Host", value: "None named"))
  }
}
