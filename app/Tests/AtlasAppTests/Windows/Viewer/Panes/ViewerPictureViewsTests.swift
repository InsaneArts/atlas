import AsyncSupportTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@MainActor
@Suite("ViewerPictureViews")
struct ViewerPictureViewsTests {
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
  ///   - shotOutlet: Where copied pictures go and how files are shown.
  /// - Returns: The viewer, its log read.
  /// - Throws: The failed requirement, when the log is never read.
  static func onActivity(
    _ service: ScriptedService,
    clock: ManualClock = ManualClock(),
    shotOutlet: ShotOutlet = ShotOutlet(copy: { _, _ in true }, reveal: { _ in true })
  ) async throws -> ViewerController {
    let click = ActionFacts(action: .click, display: .init(width: 4, height: 3), point: .init(x: 2, y: 1))
    service.activityLog = ActivityLog(entries: [
      ActivityLog.Entry(
        id: "c2",
        says: "clicked in TextEdit",
        ageSeconds: 1,
        app: "TextEdit",
        label: "Send",
        before: names.second,
        after: names.after,
        facts: click
      ),
      ActivityLog.Entry(
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
    let viewer = try await ViewerController.watching(service, clock: clock, shotOutlet: shotOutlet)
    viewer.send(.show(.activity))
    try await settle("the log was never read") { viewer.panes.activity.log != nil }
    return viewer
  }

  @Test("a step's picture opens in the sheet, reads both its pictures as taken, and lets them go on Done")
  func openReadsBothPictures() async throws {
    let service = ScriptedService()
    let viewer = try await Self.onActivity(service)
    defer { viewer.close() }
    viewer.send(.openAction("c2"))
    try await settle("the pictures were never read") { viewer.panes.activity.pictures.count == 2 }
    let sheet = try #require(ViewerPresenter.activity(viewer.snapshot).sheet)
    #expect(sheet.id == "c2")
    #expect(sheet.hasAfter)
    #expect(sheet.picture?.id == "\(Self.names.second) full")
    viewer.send(.pictureSide(.after))
    #expect(ViewerPresenter.activity(viewer.snapshot).sheet?.picture?.id == "\(Self.names.after) full")

    viewer.send(.previousPicture)
    let previous = try #require(ViewerPresenter.activity(viewer.snapshot).sheet)
    #expect(previous.id == "c1")
    #expect(previous.side == .before, "a step stepped to shows the screen before it first")

    viewer.send(.closePicture)
    #expect(ViewerPresenter.activity(viewer.snapshot).sheet == nil)
    #expect(viewer.panes.activity.pictures.isEmpty)
  }

  @Test("a sheet or replay whose run is gone is let go, and their keys go back to the window")
  func staleFocusLetGo() async throws {
    let service = ScriptedService()
    let viewer = try await Self.onActivity(service)
    defer { viewer.close() }
    viewer.send(.openAction("c2"))
    #expect(ViewerPresenter.shownOverlays(viewer.snapshot).sheet)
    #expect(ActionKeys.event(keyCode: ActionKeys.before, hasModifiers: false, sheet: true, replaying: false) != nil)

    viewer.panesModel.panes.activity.sheet = ViewerPictureFocus(run: "gone", item: .before("c2"), everyStep: true)
    #expect(ViewerPresenter.shownOverlays(viewer.snapshot) == (false, false), "nothing shows, so no key is caught")
    viewer.panesModel.pictures.refresh()
    #expect(viewer.panes.activity.sheet == nil)

    viewer.panesModel.panes.activity.replay = ViewerReplayFocus(moment: "gone")
    viewer.panesModel.pictures.refresh()
    #expect(viewer.panes.activity.replay == nil)
  }

  @Test("a replay plays one frame a second from its first, stops on the screen after, and plays again from the start")
  func replayPlays() async throws {
    let service = ScriptedService()
    let clock = ManualClock()
    let viewer = try await Self.onActivity(service, clock: clock)
    defer { viewer.close() }
    let moment = try #require(ViewerActivityFixtures.runs(ViewerPresenter.activity(viewer.snapshot)).first)
    #expect(moment.canReplay)
    viewer.send(.openReplay(moment.id))
    #expect(ViewerPresenter.activity(viewer.snapshot).replay?.frames.count == 3)
    // Half a second along, so the replay's waits never end with the Activity reading's, every two seconds.
    clock.advance(by: .milliseconds(500))
    viewer.send(.togglePlay)
    for index in 1...2 {
      try await clock.untilSleeping(for: ViewerPictureViews.hold, "the replay never waited on its frame")
      clock.advance(by: ViewerPictureViews.hold)
      try await settle("the replay never moved on") {
        ViewerPresenter.activity(viewer.snapshot).replay?.index == index
      }
    }
    try await clock.untilSleeping(for: ViewerPictureViews.hold, "the replay never waited on its last frame")
    clock.advance(by: ViewerPictureViews.hold)
    try await settle("the replay never stopped") {
      ViewerPresenter.activity(viewer.snapshot).replay?.isPlaying == false
    }
    #expect(ViewerPresenter.activity(viewer.snapshot).replay?.index == 2)

    viewer.send(.togglePlay)
    let replay = try #require(ViewerPresenter.activity(viewer.snapshot).replay)
    #expect(replay.isPlaying && replay.index == 0, "Play on the last frame starts again")
    viewer.send(.closeReplay)
    #expect(viewer.panesModel.pictures.playing == nil)
  }

  @Test("the replay stops playing while the window leaves Activity, and the frames step by hand")
  func replayStopsOffActivity() async throws {
    let service = ScriptedService()
    let viewer = try await Self.onActivity(service)
    defer { viewer.close() }
    let moment = try #require(ViewerActivityFixtures.runs(ViewerPresenter.activity(viewer.snapshot)).first)
    viewer.send(.openReplay(moment.id))
    viewer.send(.nextFrame)
    viewer.send(.nextFrame)
    viewer.send(.nextFrame)
    #expect(viewer.panes.activity.replay?.index == 2, "never past the last frame")
    viewer.send(.replayFrame(0))
    viewer.send(.previousFrame)
    #expect(viewer.panes.activity.replay?.index == 0, "never before the first")
    viewer.send(.togglePlay)
    #expect(viewer.panesModel.pictures.playing != nil)
    viewer.send(.show(.apps))
    #expect(viewer.panesModel.pictures.playing == nil)
    #expect(viewer.panesModel.pictures.loads.isEmpty)
  }
}
