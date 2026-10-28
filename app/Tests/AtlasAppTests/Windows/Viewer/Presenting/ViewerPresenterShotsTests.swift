import AtlasService
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("ViewerPresenter+Shots")
struct ViewerPresenterShotsTests {
  @Test("opening a screenshot zooms it from its own tile")
  func openingZoomsFromItsOwnTile() throws {
    var snapshot = ViewerSnapshot()
    snapshot.panes.shots.shots = ["a.png", "b.png", "c.png"]
      .enumerated()
      .map { index, name in
        Screenshot(
          name: name,
          takenAt: snapshot.date.addingTimeInterval(Double(-60 * index)),
          width: 2560,
          height: 1440
        )
      }
    snapshot.panes.shots.large = "b.png"
    let shots = ViewerPresenter.shots(snapshot)
    let large = try #require(shots.large)
    // The large view and the tile it opened from carry one identity, which their picture's geometry shares; no other
    // tile carries it.
    let tiles = shots.days.flatMap(\.shots)
    #expect(large.id == "b.png")
    let carrying =
      tiles
      .map(\.id)
      .filter { $0 == large.id }
    #expect(carrying.count == 1)
  }

  @Test("screenshots group as Activity's lines do: the last hour is Now and the rest of today one group")
  func screenshotsGroupAsActivityDoes() throws {
    var snapshot = ViewerSnapshot()
    snapshot.timeZone = TimeZone(secondsFromGMT: 0) ?? .current
    // 17:30 UTC: a quarter of an hour ago, the afternoon and the morning.
    snapshot.date = Date(timeIntervalSince1970: 1_792_690_200)
    snapshot.panes.shots.shots = [-900.0, -4 * 3600, -8 * 3600]
      .enumerated()
      .map { index, ago in Screenshot(name: "\(index).png", takenAt: snapshot.date.addingTimeInterval(ago)) }
    let shots = ViewerPresenter.shots(snapshot)
    #expect(shots.days.map(\.title) == ["Now", "Earlier today"])
    #expect(shots.days.map { $0.shots.map(\.id) } == [["0.png"], ["1.png", "2.png"]])
    #expect(shots.meta == "3 today", "the count alone; how to use a tile is in its tooltip")
  }

  @Test("selection and large screenshot navigation follow the grid's newest first order")
  func selectionAndNavigationFollowTheGridOrder() throws {
    var snapshot = ViewerSnapshot()
    snapshot.panes.shots.shots = [
      Screenshot(name: "oldest.png", takenAt: snapshot.date.addingTimeInterval(-7200)),
      Screenshot(name: "newest.png", takenAt: snapshot.date),
      Screenshot(name: "middle.png", takenAt: snapshot.date.addingTimeInterval(-60)),
    ]
    snapshot.panes.shots.selected = "missing.png"
    for (name, back, forward) in [
      ("newest.png", false, true), ("middle.png", true, true), ("oldest.png", true, false),
    ] {
      snapshot.panes.shots.large = name
      let shots = ViewerPresenter.shots(snapshot)
      let tiles = shots.days.flatMap(\.shots)
      #expect(tiles.map(\.id) == ["newest.png", "middle.png", "oldest.png"])
      let selected =
        tiles
        .filter(\.isSelected)
        .map(\.id)
      #expect(selected == ["newest.png"])
      let large = try #require(shots.large)
      #expect(large.id == name)
      #expect(large.canGoBack == back)
      #expect(large.canGoForward == forward)
    }
    snapshot.panes.shots.selected = "middle.png"
    let selected = ViewerPresenter.shots(snapshot).days
      .flatMap(\.shots)
      .filter(\.isSelected)
      .map(\.id)
    #expect(selected == ["middle.png"])
    snapshot.panes.shots.selectionCleared = true
    let cleared = ViewerPresenter.shots(snapshot).days
      .flatMap(\.shots)
    #expect(!cleared.contains { $0.isSelected })
  }

  @Test("with no screenshot yet the header says nothing beside its title")
  func noScreenshotsLeaveTheHeaderQuiet() {
    var snapshot = ViewerSnapshot()
    snapshot.panes.shots.shots = []
    let shots = ViewerPresenter.shots(snapshot)
    #expect(shots.meta.isEmpty)
    #expect(shots.empty?.headline == ViewerWords.noShots)
  }

  @Test("with nothing in the log or no app open, the header has no count and there is nothing to search")
  func emptyPanesHaveNoCountAndNoSearch() {
    var snapshot = ViewerSnapshot()
    snapshot.panes.activity.log = ActivityLog()
    snapshot.panes.activity.readAt = snapshot.date
    let activity = ViewerPresenter.activity(snapshot)
    #expect(activity.meta.isEmpty)
    #expect(!activity.isSearchable)
    #expect(activity.empty?.headline == ViewerWords.noActivity)
    snapshot.panes.apps.apps = []
    let apps = ViewerPresenter.apps(snapshot)
    #expect(apps.meta.isEmpty)
    #expect(!apps.isSearchable)
  }

  @Test("a chosen session shows only its own screenshots")
  func aChosenSessionShowsOnlyItsOwn() throws {
    var snapshot = try ViewerSnapshot.listing(ViewerPresenterSessionsTests.three)
    snapshot.panes.shots.shots = [
      Screenshot(name: "mine.png", takenAt: snapshot.date, session: "3f9a1c07b2e4"),
      Screenshot(name: "listed.png", takenAt: snapshot.date.addingTimeInterval(-30)),
      Screenshot(name: "codex.png", takenAt: snapshot.date.addingTimeInterval(-60), session: "8be20c4d1a77"),
    ]
    snapshot.panes.sessions.list = HostSessionList(sessions: [
      HostSessionRecord(
        key: "3f9a1c07b2e4",
        name: "Claude Code · understudy",
        host: .claude,
        screenshots: ["listed.png"]
      )
    ])
    #expect(ViewerPresenter.shotOrder(snapshot).count == 3, "every session's while all show")
    snapshot.session.sessionChoice = .one("3f9a1c07b2e4")
    let shots = ViewerPresenter.shots(snapshot)
    let names = shots.days
      .flatMap(\.shots)
      .map(\.id)
    #expect(names == ["mine.png", "listed.png"])
    #expect(shots.meta == "2 by Claude Code · understudy")
    // The large view steps through the session's own only.
    #expect(ViewerPresenter.shotOrder(snapshot) == ["mine.png", "listed.png"])
  }
}
