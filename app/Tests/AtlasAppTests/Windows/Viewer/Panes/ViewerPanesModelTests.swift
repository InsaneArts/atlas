import AsyncSupportTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("ViewerPanesModel")
struct ViewerPanesModelTests {
  /// Two screenshots, newest first, as the service lists them.
  static let newer = "1792665104000000000-capture.png"

  /// The older of the two.
  static let older = "1792665080000000000-capture.png"

  /// The two screenshots, with their times.
  static let shots = [
    Screenshot(name: newer, takenAt: Date(timeIntervalSince1970: 1_792_665_104)),
    Screenshot(name: older, takenAt: Date(timeIntervalSince1970: 1_792_665_080)),
  ]

  /// Waits, within the test's ceiling, for a task to end.
  ///
  /// - Parameters:
  ///   - task: The task; nil has ended already.
  ///   - message: What never came, as the failure says it.
  /// - Throws: The failed requirement, once the ceiling passes with the task still running.
  static func ended(_ task: Task<Void, Never>?, _ message: String) async throws {
    try await task?.value(message)
  }

  /// Lets the oldest held screenshot request for `name` at that size answer, once it has been made.
  ///
  /// - Parameters:
  ///   - service: The scripted service.
  ///   - name: The screenshot asked for.
  ///   - full: Whether it was asked for as taken.
  /// - Throws: The failed requirement, when the request is never made.
  static func answer(_ service: ScriptedService, _ name: String, full: Bool) async throws {
    try await settle("\(name) was never asked for") {
      service.heldShots.contains { $0.name == name && $0.full == full && !$0.answered }
    }
    service.resumeShot(name: name, full: full)
  }

  /// A viewer on the Screenshots tab over the two screenshots, its every screenshot request held.
  ///
  /// - Parameters:
  ///   - service: The scripted service.
  ///   - shotOutlet: Where copied screenshots go and how files are shown.
  /// - Returns: The viewer, its screenshots read.
  /// - Throws: The failed requirement, when they never are.
  static func onShots(
    _ service: ScriptedService,
    shotOutlet: ShotOutlet = ShotOutlet(copy: { _ in }, reveal: { _ in })
  ) async throws -> ViewerController {
    service.shots = shots
    let viewer = try await ViewerController.watching(
      service,
      shotOutlet: shotOutlet
    )
    service.holdShots = true
    viewer.send(.show(.screenshots))
    try await settle("the screenshots were never read") { viewer.panes.shots.shots != nil }
    return viewer
  }

  @Test("Activity stops reading when the window hides or leaves its tab")
  func activityStopsReading() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    viewer.send(.show(.activity))
    try await settle("Activity was never read") { service.activityCalls == 1 }
    let first = viewer.panesModel.work.reading
    viewer.send(.show(.apps))
    try await Self.ended(first, "Activity's reading went on after its tab was left")
    #expect(service.activityCalls == 1)
    viewer.send(.show(.activity))
    try await settle("Activity was not read on its return") { service.activityCalls == 2 }
    let second = viewer.panesModel.work.reading
    viewer.setVisible(false)
    try await Self.ended(second, "Activity's reading went on after the window hid")
    #expect(service.activityCalls == 2)
    viewer.close()
  }

  @Test("each row's time counts back from when the log arrived, never from after the screenshots' reply")
  func activityTimesCountFromTheLog() async throws {
    let service = ScriptedService()
    service.activityLog = ActivityLog(entries: [
      ActivityLog.Entry(id: "1", says: "took a screenshot", ageSeconds: 4, operation: "screenshot")
    ])
    var now = Date(timeIntervalSince1970: 1_000)
    let viewer = try await ViewerController.watching(
      service,
      wallClock: { now }
    )
    service.hold = ["screenshots"]
    viewer.send(.show(.activity))
    try await settle("the screenshots were never asked for") { service.held["screenshots"] != nil }
    now = Date(timeIntervalSince1970: 1_030)
    service.resume("screenshots")
    try await settle("the log was never kept") { viewer.panes.activity.readAt != nil }
    #expect(viewer.panes.activity.readAt == Date(timeIntervalSince1970: 1_000))
    viewer.close()
  }

  @Test("a stale thumbnail reply never replaces a newer one")
  func staleThumbnailNeverLands() async throws {
    let service = ScriptedService()
    let viewer = try await Self.onShots(service)
    service.shot = png(width: 1, height: 1)
    viewer.send(.thumbnailNeeded(Self.newer))
    let stale = viewer.panesModel.work.thumbnails.loading[Self.newer]
    try await settle("the first thumbnail was never asked for") { service.heldShots.count == 1 }
    viewer.send(.show(.screen))
    viewer.send(.show(.screenshots))
    service.shot = png(width: 2, height: 2)
    viewer.send(.thumbnailNeeded(Self.newer))
    let fresh = viewer.panesModel.work.thumbnails.loading[Self.newer]
    try await settle("the requests were never made") { service.heldShots.count == 2 }
    service.heldShots[1].answered = true
    service.heldShots[1].resume.resume()
    try await Self.ended(fresh, "the newer thumbnail never arrived")
    try #require(viewer.panes.thumbnails[Self.newer]?.image.width == 2, "the newer thumbnail was kept")
    try await Self.answer(service, Self.newer, full: false)
    try await Self.ended(stale, "the stale reply never came back")
    #expect(viewer.panes.thumbnails[Self.newer]?.image.width == 2)
    viewer.close()
  }

  @Test("leaving the screenshots for Apps lets the thumbnail queue go, so a reply starts no queued request")
  func appsLetsTheThumbnailsGo() async throws {
    let service = ScriptedService()
    let viewer = try await Self.onShots(service)
    for name in ["1-capture.png", "2-capture.png", "3-capture.png"] { viewer.send(.thumbnailNeeded(name)) }
    try await settle("two thumbnails were never asked for") { service.heldShots.count == 2 }
    let first = viewer.panesModel.work.thumbnails.loading["1-capture.png"]
    viewer.send(.show(.apps))
    try await Self.answer(service, "1-capture.png", full: false)
    try await Self.ended(first, "the first thumbnail's reply never came back")
    #expect(service.shotCalls == 2, "the third thumbnail was never asked for")
    service.resumeAllShots()
    viewer.close()
  }

  @Test("pending thumbnails resume when the same pane shows again", arguments: [ViewerPlace.screenshots, .activity])
  func hiddenThumbnailsResume(_ place: ViewerPlace) async throws {
    let service = ScriptedService()
    let viewer = try await Self.onShots(service)
    defer {
      viewer.close()
      service.resumeAllShots()
    }
    let queued = "queued.png"
    let removed = "removed.png"
    service.shots += [Screenshot(name: queued), Screenshot(name: removed)]
    await viewer.panesModel.readShots(viewer.panesModel.work.generation)
    if place == .activity {
      service.activityLog = ActivityLog(entries: [
        ActivityLog.Entry(id: "1", says: "took a screenshot", ageSeconds: 4, operation: "screenshot")
      ])
      viewer.send(.show(.activity))
      try await settle("Activity was never read") { viewer.panes.activity.readAt != nil }
    }
    service.shot = png(width: 1, height: 1)
    for name in [Self.newer, Self.older, queued, removed] { viewer.send(.thumbnailNeeded(name)) }
    try await settle("two thumbnails were never asked for") { service.heldShots.count == 2 }
    let stale = Array(viewer.panesModel.work.thumbnails.loading.values)
    viewer.setVisible(false)
    viewer.setVisible(false)
    for task in stale { #expect(task.isCancelled) }
    #expect(viewer.panesModel.work.thumbnails.loading.isEmpty)
    service.shots.removeAll { $0.name == removed }
    await viewer.panesModel.readShots(viewer.panesModel.work.generation)
    service.shot = png(width: 2, height: 2)
    viewer.setVisible(true)
    try await settle("the cancelled thumbnails were not requested again") { service.heldShots.count == 4 }
    let fresh = Array(viewer.panesModel.work.thumbnails.loading.values)
    for name in [Self.newer, Self.older] { try await Self.answer(service, name, full: false) }
    for task in stale { try await Self.ended(task, "a stale thumbnail never replied") }
    #expect(viewer.panes.thumbnails[Self.newer] == nil)
    #expect(viewer.panes.thumbnails[Self.older] == nil)
    #expect(viewer.panesModel.work.thumbnails.loading.count == 2)
    #expect(service.heldShots.count == 4, "late replies do not start the queued thumbnail")
    for name in [Self.newer, Self.older] { try await Self.answer(service, name, full: false) }
    for task in fresh { try await Self.ended(task, "a resumed thumbnail never arrived") }
    try await settle("the queued thumbnail was never requested") { service.heldShots.count == 5 }
    let last = viewer.panesModel.work.thumbnails.loading[queued]
    try await Self.answer(service, queued, full: false)
    try await Self.ended(last, "the queued thumbnail never arrived")
    for name in [Self.newer, Self.older, queued] {
      #expect(viewer.panes.thumbnails[name]?.image.width == 2)
    }
    #expect(viewer.panesModel.work.thumbnails.wanted.isEmpty)
    #expect(!service.heldShots.contains { $0.name == removed })
    viewer.setVisible(false)
    viewer.setVisible(true)
    #expect(viewer.panesModel.work.thumbnails.loading.isEmpty, "cached thumbnails need no new requests")
  }

  @Test(
    "a screenshot removed from the list closes its large view and leaves exports and arrows on the grid",
    arguments: [false, true]
  )
  func removedLargeShotCloses(_ loaded: Bool) async throws {
    let service = ScriptedService()
    let newest = "1792665120000000000-capture.png"
    service.shotData = [newest: png(width: 1, height: 1), Self.older: png(width: 2, height: 2)]
    var copied: [Data] = []
    var saved: [Data] = []
    let viewer = try await Self.onShots(
      service,
      shotOutlet: ShotOutlet(
        copy: { copied.append($0) },
        reveal: { _ in },
        save: { data, _ in
          saved.append(data)
          return true
        }
      )
    )
    defer {
      viewer.close()
      service.resumeAllShots()
    }
    viewer.send(.openShot(Self.older))
    let load = try #require(viewer.panesModel.work.large.load)
    if loaded {
      try await Self.answer(service, Self.older, full: true)
      try await Self.ended(load, "the large screenshot never arrived")
      try #require(viewer.panes.shots.full != nil)
    }
    service.shots = [Screenshot(name: newest, takenAt: Date(timeIntervalSince1970: 1_792_665_120)), Self.shots[0]]
    await viewer.panesModel.readShots(viewer.panesModel.work.generation)
    #expect(viewer.panes.shots.large == nil)
    #expect(viewer.panes.shots.selected == nil)
    #expect(viewer.panes.shots.full == nil)
    #expect(viewer.panesModel.work.large.load == nil)
    if !loaded {
      #expect(load.isCancelled)
      try await Self.answer(service, Self.older, full: true)
      try await Self.ended(load, "the removed screenshot never replied")
      #expect(viewer.panes.shots.full == nil)
    }
    #expect(ViewerPresenter.selectedShot(viewer.snapshot) == newest)
    #expect(viewer.panesModel.actedShot == newest)
    viewer.send(.copyShot)
    let copy = viewer.panesModel.work.large.copy
    try await Self.answer(service, newest, full: true)
    try await Self.ended(copy, "Copy never finished")
    viewer.panesModel.saveShot()
    let save = viewer.panesModel.work.large.copy
    try await Self.answer(service, newest, full: true)
    try await Self.ended(save, "Save never finished")
    #expect(copied == [service.shotData[newest]])
    #expect(saved == [service.shotData[newest]])
    viewer.send(.nextShot)
    #expect(ViewerPresenter.selectedShot(viewer.snapshot) == Self.newer)
    #expect(viewer.panesModel.actedShot == Self.newer)
    service.shots.removeAll { $0.name == Self.newer }
    await viewer.panesModel.readShots(viewer.panesModel.work.generation)
    #expect(viewer.panes.shots.selected == nil)
    #expect(viewer.panesModel.actedShot == newest)
  }

  @Test("an older Copy never lands over a newer one, read or kept")
  func olderCopyNeverLands() async throws {
    let service = ScriptedService()
    service.shotData = [Self.newer: png(width: 1, height: 1), Self.older: png(width: 2, height: 2)]
    var copied: [Data] = []
    let viewer = try await Self.onShots(service, shotOutlet: ShotOutlet(copy: { copied.append($0) }, reveal: { _ in }))
    viewer.send(.openShot(Self.newer))
    viewer.send(.copyShot)
    let firstCopy = viewer.panesModel.work.large.copy
    viewer.send(.nextShot)
    let olderLoad = viewer.panesModel.work.large.load
    viewer.send(.copyShot)
    let secondCopy = viewer.panesModel.work.large.copy
    try await Self.answer(service, Self.older, full: true)
    try await Self.ended(olderLoad, "the older screenshot never arrived")
    viewer.send(.copyShot)
    #expect(copied == [service.shotData[Self.older]], "the kept picture is copied at once")
    try await Self.answer(service, Self.older, full: true)
    try await Self.ended(secondCopy, "the second Copy's reply never came back")
    try await Self.answer(service, Self.newer, full: true)
    try await Self.answer(service, Self.newer, full: true)
    try await Self.ended(firstCopy, "the first Copy's reply never came back")
    #expect(copied == [service.shotData[Self.older]])
    service.resumeAllShots()
    viewer.close()
  }

  @Test("a delayed screenshot reply after close does not run the save outlet")
  func delayedSaveDoesNotRunAfterClose() async throws {
    let service = ScriptedService()
    service.shot = png()
    var saved = false
    let viewer = try await Self.onShots(
      service,
      shotOutlet: ShotOutlet(
        copy: { _ in },
        reveal: { _ in },
        save: { _, _ in
          saved = true
          return true
        }
      )
    )
    viewer.send(.selectShot(Self.newer))
    viewer.panesModel.saveShot()
    let save = try #require(viewer.panesModel.work.large.copy)
    try await settle("the screenshot was never asked for") { service.heldShots.count == 1 }
    viewer.close()
    try await Self.answer(service, Self.newer, full: true)
    try await Self.ended(save, "the screenshot's reply never came back")
    #expect(!saved)
    #expect(save.isCancelled)
    #expect(viewer.panesModel.work.large.copy == nil)
  }

  @Test("a large screenshot read while the window hides never lands, and is read again once it shows")
  func largeReadStopsWhileHidden() async throws {
    let service = ScriptedService()
    let viewer = try await Self.onShots(service)
    viewer.send(.openShot(Self.newer))
    let hiddenLoad = viewer.panesModel.work.large.load
    viewer.setVisible(false)
    try await Self.answer(service, Self.newer, full: true)
    try await Self.ended(hiddenLoad, "the picture's reply never came back")
    #expect(viewer.panes.shots.full == nil)
    viewer.setVisible(true)
    let shownLoad = try #require(viewer.panesModel.work.large.load, "the picture is read again")
    try await Self.answer(service, Self.newer, full: true)
    try await Self.ended(shownLoad, "the picture read again never arrived")
    #expect(viewer.panes.shots.full != nil)
    service.resumeAllShots()
    viewer.close()
  }

  @Test("closing the window drops the large screenshot still being read")
  func largeReadStopsOnClose() async throws {
    let service = ScriptedService()
    let viewer = try await Self.onShots(service)
    viewer.send(.openShot(Self.newer))
    let load = viewer.panesModel.work.large.load
    viewer.close()
    try await Self.answer(service, Self.newer, full: true)
    try await Self.ended(load, "the picture's reply never came back")
    #expect(viewer.panes.shots.full == nil)
    service.resumeAllShots()
  }

  @Test("a large screenshot that could not be read is read again with the next reading of the screenshots")
  func failedLargeReadIsTriedAgain() async throws {
    let service = ScriptedService()
    service.shots = Self.shots
    service.fail = ["screenshot"]
    let viewer = try await ViewerController.watching(service)
    viewer.send(.show(.screenshots))
    try await settle("the screenshots were never read") { viewer.panes.shots.shots != nil }
    viewer.send(.openShot(Self.newer))
    try await Self.ended(viewer.panesModel.work.large.load, "the failed read never ended")
    #expect(viewer.panesModel.work.large.load == nil)
    service.fail = []
    service.shot = png()
    await viewer.panesModel.readShots(viewer.panesModel.work.generation)
    let retry = try #require(viewer.panesModel.work.large.load, "the next reading reads it again")
    try await Self.ended(retry, "the second read never ended")
    #expect(viewer.panes.shots.full != nil)
    viewer.close()
  }

  @Test("Show in Finder shows only a listed screenshot, inside the folder the service names")
  func showInFinderRevealsTheListedFile() async throws {
    let service = ScriptedService()
    service.shotsFolder = URL(fileURLWithPath: "/Users/me/runs", isDirectory: true)
    var revealed: [URL] = []
    let viewer = try await Self.onShots(
      service,
      shotOutlet: ShotOutlet(copy: { _ in }, reveal: { revealed.append($0) })
    )
    viewer.send(.openShot(Self.newer))
    guard case .screenshots(let shots) = viewer.state.pane else {
      Issue.record("the Screenshots tab does not show")
      return
    }
    #expect(shots.large?.canShowInFinder == true)
    viewer.send(.showShotInFinder)
    #expect(revealed.map(\.path) == ["/Users/me/runs/\(Self.newer)"])
    service.shotsFolder = nil
    await viewer.panesModel.readShots(viewer.panesModel.work.generation)
    try #require(viewer.panes.shots.folder == nil, "a reading that names no folder leaves it unknown")
    viewer.send(.showShotInFinder)
    #expect(revealed.count == 1, "no file is shown while the folder is unknown")
    service.resumeAllShots()
    viewer.close()
  }

  @Test("Clear removes the outline and nothing else")
  func clearRemovesOnlyTheOutline() async throws {
    let service = ScriptedService()
    service.openApps = [
      HelperApp(
        pid: 7,
        name: "Opie",
        windows: [HelperWindow(id: 3, title: "Ring settings", x: 100, y: 80, width: 520, height: 860)]
      )
    ]
    let viewer = try await ViewerController.watching(service)
    viewer.send(.show(.apps))
    try await settle("the apps were never read") { viewer.panes.apps.apps != nil }
    viewer.send(.show(.screen))
    let before = viewer.state
    viewer.send(.showWindow(app: 7, window: 3))
    try #require(viewer.state.screen.outline != nil, "the window was outlined")
    viewer.send(.clearOutline)
    #expect(viewer.state == before)
    viewer.close()
  }

  @Test("Open on an Activity line shows its screenshot large on the Screenshots tab")
  func openShowsTheScreenshotLarge() async throws {
    let service = ScriptedService()
    service.shots = Self.shots
    service.shot = png()
    let viewer = try await ViewerController.watching(service)
    viewer.send(.show(.screenshots))
    try await settle("the screenshots were never read") { viewer.panes.shots.shots != nil }
    viewer.send(.show(.activity))
    viewer.send(.openShot(Self.newer))
    #expect(viewer.state.pane.place == .screenshots)
    guard case .screenshots(let shots) = viewer.state.pane else {
      Issue.record("the Screenshots tab does not show")
      return
    }
    #expect(shots.large?.id == Self.newer)
    viewer.close()
  }
}
