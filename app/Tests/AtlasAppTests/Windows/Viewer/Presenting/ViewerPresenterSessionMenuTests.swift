import AtlasService
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("ViewerPresenter+SessionMenu")
struct ViewerPresenterSessionMenuTests {
  /// Claude Code's session key.
  static let key = "3f9a1c07b2e4"

  /// A session's row as the sidebar draws it.
  ///
  /// - Parameters:
  ///   - key: The session's key.
  ///   - snapshot: What the controller holds.
  /// - Returns: The row.
  /// - Throws: When the sidebar draws no row for it.
  static func row(_ key: String, _ snapshot: ViewerSnapshot) throws -> ViewerSessionRow {
    try #require(
      ViewerPresenter.sessions(snapshot).groups
        .flatMap(\.rows)
        .first { $0.id == key }
    )
  }

  /// The titles of every item of a menu, each section's in order.
  ///
  /// - Parameter menu: The menu's sections.
  /// - Returns: The titles.
  static func titles(_ menu: [ViewerMenuSection]) -> [String] {
    menu.flatMap(\.items)
      .map(\.title)
  }

  /// An item of a menu, by its title.
  ///
  /// - Parameters:
  ///   - title: The item's title.
  ///   - menu: The menu's sections.
  /// - Returns: The item.
  /// - Throws: When the menu holds no such item.
  static func item(_ title: String, in menu: [ViewerMenuSection]) throws -> ViewerMenuItem {
    try #require(menu.flatMap(\.items).first { $0.title == title })
  }

  @Test("a row's tooltip lists its host, folder, start and last activity")
  func aRowsTooltipListsItsFacts() throws {
    var snapshot = try ViewerSnapshot.listing(ViewerPresenterSessionsTests.three)
    snapshot.panes.sessions.list = HostSessionList(sessions: [
      HostSessionRecord(
        key: Self.key,
        name: "Claude Code · understudy",
        host: .claude,
        hostLabel: "Claude Code",
        folder: "understudy",
        startedAt: Date(timeIntervalSince1970: 9 * 3600 + 12 * 60),
        lastSeenAt: snapshot.date,
        apps: [HostSessionApp(name: "Opie", copied: true), HostSessionApp(name: "Notes")],
        copies: [HostSessionCopy(app: "Opie"), HostSessionCopy(app: "Opie")],
        plans: [HostSessionPlan(phase: .applied, canRollBack: true), HostSessionPlan(phase: .rollingBack)],
        screenshots: ["a.png", "b.png", "c.png", "d.png"]
      )
    ])
    let row = try Self.row(Self.key, snapshot)
    let lines = row.help
      .split(separator: "\n")
      .map(String.init)
    #expect(Array(lines.prefix(2)) == ["Claude Code · understudy", "Clicking Show Center in Opie"])
    // Before the Apps list has read anything, an app reads by its name alone.
    #expect(
      Array(lines.dropFirst(2)) == [
        "Host: Claude Code", "Folder: understudy", "Started: 09:12", "Last seen: just now", "Opie", "Notes",
        "Copies of Opie: 2", "Environment plan: Applied · can roll back", "Environment plan: Rolling back",
        "Screenshots: 4",
      ]
    )
    let details = try #require(row.menu.last)
    #expect(details.title == "Details")
    #expect(details.items.allSatisfy { $0.isInfo && !$0.isEnabled }, "facts cannot be chosen")
    #expect(details.items.map(\.title) == Array(lines.dropFirst(2)), "the menu lists the tooltip's facts")
    snapshot.panes.apps.apps = [HelperApp(pid: 1, name: "Opie")]
    let opened = try Self.row(Self.key, snapshot).help
    #expect(opened.contains("Opie: Open · a copy") && opened.contains("Notes: Closed"))
    // A live session the list does not name yet has the status's own facts.
    #expect(
      try Self.row("8be20c4d1a77", snapshot).help == "Codex · atlas-landing\nWaiting for the helper · 8 s\nHost: Codex"
    )
  }

  @Test("a Needs you row offers Recover")
  func aNeedsYouRowOffersRecover() throws {
    var snapshot = try ViewerSnapshot.listing(
      [SessionReplies.claudeNeedsYou, SessionReplies.cursorIdle],
      mode: "recovering",
      state: "needsYou",
      pending: 1
    )
    let asking = try Self.item("Recover…", in: Self.row(Self.key, snapshot).menu)
    #expect(asking.command == .askRecover)
    #expect(!asking.isEnabled, "the recovery alert is up, as it holds the Helper menu's Recover…")
    let offered = try #require(snapshot.offeredRecovery)
    snapshot.session.recoveryLater = RecoveryKey(offered, status: snapshot.reading.status)
    let needsYou = try Self.row(Self.key, snapshot).menu
    let recover = try Self.item("Recover…", in: needsYou)
    #expect(recover.isEnabled && snapshot.helperMenu.canRecover, "set aside, it can be chosen as the Helper menu's")
    #expect(!Self.titles(try Self.row("5d21e9a04c3f", snapshot).menu).contains("Recover…"))
    #expect(Self.titles(needsYou).prefix(2) == ["Show Activity", "Show Screenshots"])
    #expect(try Self.item("Show Screenshots", in: needsYou).command == .showScreenshots(session: Self.key))
  }

  @Test("a row offers Clean Up only when its session left work to clean up")
  func aRowOffersCleanUpOnlyWhenItsSessionLeftWork() throws {
    let offered = { (record: HostSessionRecord) throws -> ViewerMenuItem? in
      var snapshot = try ViewerSnapshot.listing(ViewerPresenterSessionsTests.three)
      snapshot.panes.sessions.list = HostSessionList(sessions: [record])
      return try Self.row(record.key, snapshot).menu
        .flatMap(\.items)
        .first { $0.command == .askCleanup(session: record.key) }
    }
    typealias Records = ViewerPresenterCleanupTests
    #expect(try offered(Records.record()) == nil, "a session that left nothing offers none")
    #expect(try offered(Records.record(cleanup: .complete)) == nil, "a cleanup that finished leaves nothing")
    let copied = try #require(try offered(Records.record(copies: [HostSessionCopy(app: "Opie")])))
    #expect(copied.title == "Clean Up This Session's Work…")
    #expect(copied.role == .destructive)
    #expect(copied.isEnabled)
    #expect(try offered(Records.record(installs: ["Probe"])) != nil, "an app its task installed is left")
  }

  @Test("Send Key in a row's menu is enabled only while in control")
  func sendKeyInARowsMenuIsEnabledOnlyInControl() throws {
    var snapshot = try ViewerSnapshot.listing(ViewerPresenterSessionsTests.three)
    let watching = try Self.item("Send Key", in: Self.row(Self.key, snapshot).menu)
    #expect(!watching.isEnabled)
    #expect(watching.children.allSatisfy { !$0.isEnabled })
    #expect(watching.submenu.map(\.title) == ["Mac Shortcuts", "App Shortcuts"])
    snapshot.lease = "lease"
    let holding = try Self.item("Send Key", in: Self.row(Self.key, snapshot).menu)
    #expect(holding.isEnabled)
    #expect(holding.children.map(\.title) == HelperKey.all.map(\.title))
    #expect(holding.children.map(\.command) == HelperKey.all.indices.map { .sendKey(index: $0) })
    #expect(holding.children.allSatisfy { $0.isEnabled })
  }

  @Test("a sessions' list that cannot be read says so in the session's facts")
  func anUnreadListSaysSo() throws {
    var snapshot = try ViewerSnapshot.listing([SessionReplies.claudeActing, SessionReplies.terminalIdle])
    snapshot.panes.sessions.list = HostSessionList(sessions: [
      HostSessionRecord(key: "", name: "Terminal", host: .terminal)
    ])
    #expect(!(try Self.row("", snapshot).help.contains("Sessions:")))
    snapshot.panes.sessions.failure = "The service did not answer"
    #expect(try Self.row("", snapshot).help.hasSuffix("Sessions: Couldn't read sessions; this may be out of date"))
  }

  @Test("Terminal's host is none named")
  func terminalsHostIsNoneNamed() throws {
    let snapshot = try ViewerSnapshot.listing([SessionReplies.claudeActing, SessionReplies.terminalIdle])
    #expect(try Self.row("", snapshot).help.contains("Host: None named"))
    #expect(!(try Self.row("", snapshot).menu.flatMap(\.items).contains { $0.role == .destructive }))
  }
}
