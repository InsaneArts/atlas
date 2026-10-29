import AtlasService
import AtlasStatus
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("ViewerPresenter+Apps")
struct ViewerPresenterAppsTests {
  /// Claude Code's session key.
  static let claude = "3f9a1c07b2e4"
  /// Codex's session key.
  static let codex = "8be20c4d1a77"

  /// Opie, which Claude Code ran from a copy 4 minutes before `now` and which is in front, its front window whole on the
  /// sample display.
  ///
  /// - Parameter now: The time now.
  /// - Returns: The app.
  static func opie(_ now: Date) -> HelperApp {
    HelperApp(
      pid: 501,
      name: "Opie",
      front: true,
      launchedAt: now.addingTimeInterval(-240),
      origin: HelperAppOrigin(session: claude, how: .copy, at: now.addingTimeInterval(-240)),
      windows: [
        HelperWindow(id: 12, title: "Ring settings", x: 230, y: 108, width: 806, height: 630, order: 0),
        HelperWindow(id: 11, title: "Starter", x: 300, y: 160, width: 1080, height: 590, order: 1),
      ]
    )
  }

  /// Notes, which Codex opened half a minute before `now`, hidden.
  ///
  /// - Parameter now: The time now.
  /// - Returns: The app.
  static func notes(_ now: Date) -> HelperApp {
    HelperApp(
      pid: 503,
      name: "Notes",
      hidden: true,
      launchedAt: now.addingTimeInterval(-30),
      origin: HelperAppOrigin(session: codex, how: .launch, at: now.addingTimeInterval(-30)),
      windows: [HelperWindow(id: 31, title: "QA checklist", x: 520, y: 200, width: 900, height: 640, order: 3)]
    )
  }

  /// Probe, which one of Claude Code's tasks installed 11 minutes before `now`.
  ///
  /// - Parameter now: The time now.
  /// - Returns: The app.
  static func probe(_ now: Date) -> HelperApp {
    HelperApp(
      pid: 506,
      name: "Probe",
      launchedAt: now.addingTimeInterval(-660),
      origin: HelperAppOrigin(session: claude, how: .install, at: now.addingTimeInterval(-660)),
      windows: [HelperWindow(id: 61, title: "Untitled", x: 160, y: 220, width: 640, height: 480, order: 2)]
    )
  }

  /// Finder, open since well before the agents, with no window.
  ///
  /// - Parameter now: The time now.
  /// - Returns: The app.
  static func finder(_ now: Date) -> HelperApp {
    HelperApp(pid: 505, name: "Finder", launchedAt: now.addingTimeInterval(-5400))
  }

  /// Claude Code, Codex and Cursor listed, the sample desktop on show, and these apps read twice with no window moved.
  ///
  /// - Parameter apps: The apps, front to back.
  /// - Returns: The snapshot.
  /// - Throws: A decoding error, which a fixed reply never gives.
  static func listing(_ apps: (Date) -> [HelperApp]) throws -> ViewerSnapshot {
    var snapshot = try ViewerSnapshot.listing(ViewerPresenterSessionsTests.three)
    snapshot.session.place = .apps
    snapshot.timeZone = TimeZone(secondsFromGMT: 0) ?? .current
    snapshot.locale = Locale(identifier: "de_DE")
    let read = apps(snapshot.date)
    snapshot.panes.apps.apps = read
    snapshot.panes.apps.steadyWindows = ViewerPanesModel.steadyWindows(read, since: read)
    return snapshot
  }

  /// Every app of the sample, front to back.
  ///
  /// - Parameter now: The time now.
  /// - Returns: The apps.
  static func every(_ now: Date) -> [HelperApp] {
    [opie(now), probe(now), notes(now), finder(now)]
  }

  @Test("an app with no windows still lists, with no window rows and no picture", arguments: ["", "Find"])
  func appWithNoWindowsLists(query: String) {
    var snapshot = ViewerSnapshot()
    snapshot.panes.apps.apps = [
      HelperApp(pid: 4, name: "Opie", windows: [HelperWindow(id: 1, title: "Starter", width: 1080, height: 590)]),
      HelperApp(pid: 5, name: "Finder"),
    ]
    snapshot.session.search.query = query
    let apps = ViewerPresenter.apps(snapshot)
    let finder = apps.cards.first { $0.id == 5 }
    #expect(finder?.windows == [])
    #expect(finder?.picture == nil)
    #expect(apps.empty == nil)
  }

  @Test(
    "while every session's apps show, those agents opened come first in the order they opened, then those open before"
  )
  func everySessionsAppsInTwoGroups() throws {
    let apps = ViewerPresenter.apps(try Self.listing(Self.every))
    #expect(apps.groups.map(\.title) == ["Opened by agents", "Already open"])
    #expect(apps.groups.map { $0.cards.map(\.name) } == [["Probe", "Opie", "Notes"], ["Finder"]])
    #expect(apps.title == "4 apps open")
    #expect(apps.detail == "4 windows")
  }

  @Test("a chosen session shows only the apps it opened, never one already open or another session's, with no heading")
  func aChosenSessionShowsOnlyItsOwnApps() throws {
    var snapshot = try Self.listing(Self.every)
    snapshot.session.chosenSession = .one(Self.claude)
    let apps = ViewerPresenter.apps(snapshot)
    #expect(apps.groups.count == 1)
    #expect(apps.groups.first?.title == nil)
    #expect(apps.cards.map(\.name) == ["Probe", "Opie"])
    #expect(apps.cards.allSatisfy { $0.origin == nil }, "the session is the one chosen, so no card names it")
    #expect(apps.cards.map(\.line) == ["Opened 11 min ago", "Opened 4 min ago"])
    #expect(apps.title == "2 apps open")
    #expect(ViewerPresenter.count(of: .apps, snapshot) == 2, "the Library counts the session's own apps")
  }

  @Test("a chosen session that opened no app says so plainly, while other apps are open")
  func aChosenSessionWithNoAppsSaysSo() throws {
    var snapshot = try Self.listing { [Self.notes($0), Self.finder($0)] }
    snapshot.session.chosenSession = .one(Self.claude)
    let apps = ViewerPresenter.apps(snapshot)
    #expect(apps.groups.isEmpty)
    #expect(apps.title.isEmpty)
    #expect(
      apps.empty
        == EmptyMessageState(headline: ViewerWords.noSessionApps, message: ViewerWords.noSessionAppsMessage)
    )
    #expect(ViewerPresenter.count(of: .apps, snapshot) == 0)
  }

  @Test("cards keep their order as the front app changes, and only the raised card and In front move")
  func cardsKeepTheirOrderAsTheFrontChanges() throws {
    var snapshot = try Self.listing(Self.every)
    let before = ViewerPresenter.apps(snapshot)
    snapshot.panes.apps.apps = snapshot.panes.apps.apps.map { apps in
      // The service lists apps front to back, so Probe coming to the front moves it first.
      var reordered = apps
      reordered.swapAt(0, 1)
      return reordered.map { app in
        var app = app
        app.front = app.name == "Probe"
        return app
      }
    }
    let after = ViewerPresenter.apps(snapshot)
    #expect(after.cards.map(\.id) == before.cards.map(\.id))
    #expect(after.cards.filter(\.isFront).map(\.name) == ["Probe"])
    #expect(after.cards.first { $0.name == "Probe" }?.tags.last?.words == "In front")
    #expect(after.cards.first { $0.name == "Opie" }?.tags.map(\.words) == ["From a copy"])
  }

  @Test("an app's tags say whether it is hidden, from a copy or installed by Atlas, and in front last")
  func tagsSayWhatTheAppIs() throws {
    let cards = ViewerPresenter.apps(try Self.listing(Self.every)).cards
    let tags = Dictionary(uniqueKeysWithValues: cards.map { ($0.name, $0.tags) })
    #expect(
      tags["Opie"]
        == [ViewerAppTag(words: "From a copy", tone: .attention), ViewerAppTag(words: "In front", tone: .accent)]
    )
    #expect(tags["Probe"] == [ViewerAppTag(words: "Installed by Atlas", tone: .attention)])
    #expect(tags["Notes"] == [ViewerAppTag(words: "Hidden", tone: .quiet)])
    #expect(tags["Finder"] == [])
  }

  @Test("an agent's card names its session and when, and one already open says since when")
  func cardsSayWhoAndWhen() throws {
    let cards = ViewerPresenter.apps(try Self.listing(Self.every)).cards
    let opie = try #require(cards.first { $0.name == "Opie" })
    #expect(opie.origin?.name == "Claude Code · understudy")
    #expect(opie.origin?.icon != nil)
    #expect(opie.line == "4 min ago")
    #expect(cards.first { $0.name == "Notes" }?.line == "just now")
    let finder = try #require(cards.first { $0.name == "Finder" })
    #expect(finder.origin == nil)
    #expect(finder.line.hasPrefix("Open since "))
    #expect(opie.windows.map(\.isFront) == [true, false], "the front window of the app in front is marked")
  }

  @Test("the front window's picture is cut from the live picture only while it is in view")
  func theFrontWindowIsCutOnlyWhileInView() throws {
    let snapshot = try Self.listing(Self.every)
    let frame = try #require(snapshot.reading.frame)
    let cards = ViewerPresenter.apps(snapshot).cards
    let opie = try #require(cards.first { $0.name == "Opie" }?.picture)
    #expect(opie.window?.id == "\(frame.id).12", "named by the frame and the window, so it is cut anew each frame")
    #expect(opie.map.windows.count == 2)
    #expect(opie.map.aspect == 1.6)
    #expect(cards.first { $0.name == "Probe" }?.picture?.window == nil, "Opie's windows cover Probe's")
    #expect(cards.first { $0.name == "Notes" }?.picture?.window == nil, "a hidden app's windows do not show")
    #expect(cards.first { $0.name == "Finder" }?.picture == nil, "no window, no picture")

    var paused = snapshot
    paused.session.paused = true
    var moving = snapshot
    moving.panes.apps.steadyWindows = []
    var old = snapshot
    old.now += Freshness.stale
    for (why, changed) in [("paused", paused), ("moving", moving), ("old", old)] {
      let picture = ViewerPresenter.apps(changed)
        .cards
        .first { $0.name == "Opie" }?
        .picture
      #expect(picture?.window == nil, "\(why)")
      #expect(picture?.map == opie.map, "\(why) shows where the windows sit")
    }
  }

  @Test("no window's picture is kept once Apps closes: no other place draws one")
  func noPictureOutlivesApps() throws {
    var snapshot = try Self.listing(Self.every)
    guard case .apps = ViewerPresenter.state(snapshot).pane else {
      Issue.record("Apps does not show")
      return
    }
    snapshot.session.place = .activity
    if case .apps = ViewerPresenter.state(snapshot).pane { Issue.record("Apps still shows") }
  }

  @Test("a window counts as steady only once two readings found it in the same place at the same size")
  func steadyWindowsStoodStill() {
    let window = HelperWindow(id: 1, x: 10, y: 20, width: 300, height: 200, order: 0)
    var moved = window
    moved.x = 40
    let app = { (window: HelperWindow) in [HelperApp(pid: 1, name: "Opie", windows: [window])] }
    #expect(ViewerPanesModel.steadyWindows(app(window), since: nil).isEmpty)
    #expect(ViewerPanesModel.steadyWindows(app(window), since: app(window)) == [1])
    #expect(ViewerPanesModel.steadyWindows(app(moved), since: app(window)).isEmpty)
  }

  @Test("the apps list's sessions name an agent's card once the status no longer lists its session")
  func theAppsListNamesItsSessions() throws {
    var snapshot = ViewerSnapshot()
    snapshot.panes.apps.apps = [Self.opie(snapshot.date)]
    snapshot.panes.apps.sessions = [Self.claude: HostSessionSummary(name: "Claude Code · atlas", host: .claude)]
    #expect(ViewerPresenter.apps(snapshot).cards.first?.origin?.name == "Claude Code · atlas")
  }
}
