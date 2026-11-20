import AsyncSupportTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("Viewer sessions on other desktops")
@MainActor
struct ViewerSessionsElsewhereTests {
  /// The session on work, the VM shown.
  static let landing = "aaaa00000001"
  /// The session on Second Login, which is not shown.
  static let notes = "bbbb00000002"

  /// Second Login, as a session on it names it.
  static let secondLogin = HostSessionDesktop(id: "second-login", key: "user", name: "second-login", kind: .user)
  /// The VM work, as a session on it names it.
  static let work = HostSessionDesktop(id: "work", key: "vm-work", name: "work", kind: .vm)

  /// What `sessions.json` lists while work is shown: atlas-landing acting there, and notes-sync typing in Notes on
  /// Second Login.
  static let list = HostSessionList(
    live: [
      ViewerStatus.HostSession(
        key: landing,
        name: "Claude Code · atlas-landing",
        host: .claude,
        hostLabel: "Claude Code",
        folder: "atlas-landing",
        state: .acting,
        latest: ViewerStatus.Activity(operation: "click", ok: true, says: "clicking “Take Control” in Opie"),
        desktop: work
      ),
      ViewerStatus.HostSession(
        key: notes,
        name: "Codex · notes-sync",
        host: .codex,
        hostLabel: "Codex",
        folder: "notes-sync",
        state: .acting,
        latest: ViewerStatus.Activity(operation: "type", ok: true, says: "typing in Notes"),
        desktop: secondLogin
      ),
    ],
    viewed: "work"
  )

  @Test("a session on another desktop says On Second Login and what it does, and offers no recovery or cleanup there")
  func aSessionOnAnotherDesktopSaysWhichDesktop() throws {
    var snapshot = try ViewerSnapshot.listing([
      hostSession(Self.landing, host: "claude", name: "Claude Code · atlas-landing", state: "acting")
    ])
    snapshot.desktops = DesktopLists.two(shown: "work")
    snapshot.panes.sessions.list = Self.list

    let working = ViewerPresenter.sessions(snapshot)
      .rows(.working)
    #expect(working.map(\.id) == [Self.landing, Self.notes], "the shown desktop's first")
    let row = try ViewerPresenterSessionMenuTests.row(Self.notes, snapshot)
    #expect(row.line == "On Second Login · Typing in Notes")
    #expect(row.help.contains("Desktop: Second Login"), "\(row.help)")
    #expect(try ViewerPresenterSessionMenuTests.row(Self.landing, snapshot).line.hasPrefix("On ") == false)
    #expect(ViewerPresenter.sessions(snapshot).liveCount == 2)
    #expect(ViewerPresenter.elsewhere(Self.notes, snapshot) == Self.secondLogin)
    #expect(ViewerPresenter.elsewhere(Self.landing, snapshot) == nil)

    snapshot.panes.sessions.list?.live[1].state = .needsYou
    let needing = try ViewerPresenterSessionMenuTests.row(Self.notes, snapshot)
    let items = ViewerPresenterSessionMenuTests.titles(needing.menu)
    #expect(!items.contains(HelperWords.recoverItem), "Recover acts on the desktop shown")
    #expect(!items.contains(ViewerWords.cleanUpSession))
    snapshot.session.chosenSession = .one(Self.notes)
    #expect(ViewerPresenter.chosenCleanup(snapshot) == nil)
  }

  @Test("with one desktop no session names a desktop, as before")
  func withOneDesktopNoSessionNamesADesktop() throws {
    var snapshot = try ViewerSnapshot.listing([
      hostSession(Self.landing, host: "claude", name: "Claude Code · atlas-landing", state: "acting")
    ])
    let before = ViewerPresenter.sessions(snapshot)
    snapshot.desktops = DesktopLists.one
    snapshot.panes.sessions.list = HostSessionList(live: [Self.list.live[0]])
    snapshot.panes.sessions.list?.live[0].desktop = nil
    #expect(ViewerPresenter.sessions(snapshot).rows(.working) == before.rows(.working))
    #expect(snapshot.panes.sessions.list?.elsewhere.isEmpty == true)
  }

  @Test("picking a session on another desktop reads its Activity there and doesn't change the view")
  func pickingASessionOnAnotherDesktopKeepsTheView() async throws {
    let service = ScriptedService()
    service.desktopList = DesktopLists.two(shown: "work")
    service.sessionList = Self.list
    service.sessionList.sessions = [
      HostSessionRecord(key: Self.landing, name: "Claude Code · atlas-landing", host: .claude, actions: 1)
    ]
    service.sessionLogs[Self.notes] = ActivityLog(entries: [
      ActivityLog.Entry(id: "notes-row", says: "typed in Notes", ageSeconds: 2, session: Self.notes, verb: "type")
    ])
    let viewer = try await ViewerController.watching(service)
    await viewer.actions.desktops.read()
    try await settle("the list was never read") { viewer.panes.sessions.list?.viewed == "work" }

    viewer.send(.chooseSession(.one(Self.notes)))
    viewer.send(.show(.activity))
    try await settle("its Activity was never read on Second Login") {
      viewer.panes.activity.scope == .session(Self.notes)
    }

    #expect(service.activitySessions.last == Self.notes)
    #expect(service.activityDesktops.last == "second-login", "read where it acts")
    #expect(viewer.panes.activity.log?.entries.map(\.id) == ["notes-row"])
    #expect(!service.log.contains { $0.hasPrefix("view") }, "nothing switched the view")
    #expect(viewer.actions.desktops.list?.viewed == "work")
    #expect(viewer.desktopMoves == viewer.actions.desktops.moves)
    #expect(viewer.state.sidebar.helper.desktops.first(where: \.isViewed)?.id == "work")

    viewer.send(.chooseSession(.one(Self.landing)))
    try await settle("the shown desktop's session was never read") {
      viewer.panes.activity.scope == .session(Self.landing)
    }
    #expect(service.activityDesktops.last == .some(nil), "one on the desktop shown reads as before")
    viewer.close()
  }
}
