import AsyncSupportTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@MainActor
@Suite("ViewerActionViews")
struct ViewerActionViewsTests {
  /// The pictures the log names, before and after.
  static let names = (
    first: "1792665001000000000-before.jpg",
    second: "1792665002000000000-before.jpg",
    after: "1792665003000000000-after.jpg"
  )

  /// A viewer on Activity over two clicks in TextEdit, each with its picture before, the second with one after.
  ///
  /// - Parameters:
  ///   - service: The scripted service.
  ///   - clock: The clock a replay plays on.
  /// - Returns: The viewer, its log read.
  /// - Throws: The failed requirement, when the log is never read.
  static func onActivity(_ service: ScriptedService, clock: ManualClock = ManualClock()) async throws
    -> ViewerController
  {
    let click = ActionFacts(action: .click, display: .init(width: 4, height: 3), point: .init(x: 2, y: 1))
    service.activityLog = ActivityLog(entries: [
      ActivityFixtures.row(
        id: "c2",
        says: "clicked in TextEdit",
        ageSeconds: 1,
        app: "TextEdit",
        label: "Send",
        before: names.second,
        after: names.after,
        facts: click
      ),
      ActivityFixtures.row(
        id: "c1",
        says: "clicked Name in TextEdit",
        ageSeconds: 3,
        app: "TextEdit",
        label: "Name",
        before: names.first,
        after: names.second,
        facts: click
      ),
    ])
    service.shotData = [
      names.first: png(width: 4, height: 3),
      names.second: png(width: 4, height: 3),
      names.after: png(width: 4, height: 3),
    ]
    let viewer = try await ViewerController.watching(service, clock: clock)
    viewer.send(.show(.activity))
    try await settle("the log was never read") { viewer.panes.activity.log != nil }
    return viewer
  }

  @Test("a step's picture opens its action large, reads both pictures as taken, and lets them go on Close")
  func openReadsBothPictures() async throws {
    let service = ScriptedService()
    let viewer = try await Self.onActivity(service)
    defer { viewer.close() }
    viewer.send(.openAction("c2"))
    try await settle("the pictures were never read") { viewer.panes.activity.pictures.count == 2 }
    let large = try #require(ViewerPresenter.activity(viewer.snapshot).large)
    #expect(large.id == "c2")
    #expect(large.before?.id == "\(Self.names.second) full")
    #expect(large.after?.id == "\(Self.names.after) full")

    viewer.send(.previousAction)
    #expect(ViewerPresenter.activity(viewer.snapshot).large?.id == "c1")
    viewer.send(.compareAction(.after))
    #expect(ViewerPresenter.activity(viewer.snapshot).large?.compare == .after)

    viewer.send(.closeAction)
    #expect(ViewerPresenter.activity(viewer.snapshot).large == nil)
    #expect(viewer.panes.activity.pictures.isEmpty)
  }

  @Test("focus whose action or replay is gone is let go, and their keys go back to the window")
  func staleFocusLetGo() async throws {
    let service = ScriptedService()
    let viewer = try await Self.onActivity(service)
    defer { viewer.close() }
    viewer.send(.openAction("c2"))
    #expect(ViewerPresenter.shownOverlays(viewer.snapshot).large)
    #expect(ActionKeys.event(keyCode: ActionKeys.before, hasModifiers: false, large: true, replaying: false) != nil)

    viewer.panesModel.panes.activity.action = ViewerActionFocus(moment: "gone", row: "c2")
    #expect(ViewerPresenter.shownOverlays(viewer.snapshot) == (false, false), "nothing shows, so no key is caught")
    viewer.panesModel.actions.refresh()
    #expect(viewer.panes.activity.action == nil)

    viewer.panesModel.panes.activity.replay = ViewerReplayFocus(moment: "gone")
    viewer.panesModel.actions.refresh()
    #expect(viewer.panes.activity.replay == nil)
  }

  @Test("Show changes compares the two pictures once both are read, and keeps only that comparison")
  func showChangesCompares() async throws {
    let service = ScriptedService()
    let viewer = try await Self.onActivity(service)
    defer { viewer.close() }
    viewer.send(.openAction("c2"))
    viewer.send(.toggleChanges)
    try await settle("the pictures were never compared") {
      ViewerPresenter.activity(viewer.snapshot).large?.changes != nil
    }
    #expect(ViewerPresenter.activity(viewer.snapshot).large?.showsChanges == true)
    #expect(viewer.panes.activity.changes.count == 1)
  }

  @Test("a replay plays one frame a second from its first, stops on the screen after, and plays again from the start")
  func replayPlays() async throws {
    let service = ScriptedService()
    let clock = ManualClock()
    let viewer = try await Self.onActivity(service, clock: clock)
    defer { viewer.close() }
    let moment = try #require(ViewerPresenter.activity(viewer.snapshot).days.first?.moments.first)
    #expect(moment.canReplay)
    viewer.send(.openReplay(moment.id))
    #expect(ViewerPresenter.activity(viewer.snapshot).replay?.frames.count == 3)
    // Half a second along, so the replay's waits never end with the Activity reading's, every two seconds.
    clock.advance(by: .milliseconds(500))
    viewer.send(.togglePlay)
    for index in 1...2 {
      try await clock.untilSleeping(for: ViewerActionViews.hold, "the replay never waited on its frame")
      clock.advance(by: ViewerActionViews.hold)
      try await settle("the replay never moved on") {
        ViewerPresenter.activity(viewer.snapshot).replay?.index == index
      }
    }
    try await clock.untilSleeping(for: ViewerActionViews.hold, "the replay never waited on its last frame")
    clock.advance(by: ViewerActionViews.hold)
    try await settle("the replay never stopped") {
      ViewerPresenter.activity(viewer.snapshot).replay?.isPlaying == false
    }
    #expect(ViewerPresenter.activity(viewer.snapshot).replay?.index == 2)

    viewer.send(.togglePlay)
    let replay = try #require(ViewerPresenter.activity(viewer.snapshot).replay)
    #expect(replay.isPlaying && replay.index == 0, "Play on the last frame starts again")
    viewer.send(.closeReplay)
    #expect(viewer.panesModel.actions.playing == nil)
  }

  @Test("the replay stops playing while the window leaves Activity, and the frames step by hand")
  func replayStopsOffActivity() async throws {
    let service = ScriptedService()
    let viewer = try await Self.onActivity(service)
    defer { viewer.close() }
    let moment = try #require(ViewerPresenter.activity(viewer.snapshot).days.first?.moments.first)
    viewer.send(.openReplay(moment.id))
    viewer.send(.nextFrame)
    viewer.send(.nextFrame)
    viewer.send(.nextFrame)
    #expect(viewer.panes.activity.replay?.index == 2, "never past the last frame")
    viewer.send(.replayFrame(0))
    viewer.send(.previousFrame)
    #expect(viewer.panes.activity.replay?.index == 0, "never before the first")
    viewer.send(.togglePlay)
    #expect(viewer.panesModel.actions.playing != nil)
    viewer.send(.show(.apps))
    #expect(viewer.panesModel.actions.playing == nil)
    #expect(viewer.panesModel.actions.loads.isEmpty)
  }
}
