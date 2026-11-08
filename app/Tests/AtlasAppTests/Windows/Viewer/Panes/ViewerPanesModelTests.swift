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

  /// The log while an agent took the two screenshots, newest first.
  static let log = ActivityLog(entries: [
    ActivityFixtures.row(id: "s2", says: "took a screenshot", ageSeconds: 4, operation: "screenshot", capture: newer),
    ActivityFixtures.row(id: "s1", says: "took a screenshot", ageSeconds: 28, operation: "screenshot", capture: older),
  ])

  /// A viewer on Activity over an agent's two screenshots, its every screenshot request held.
  ///
  /// - Parameters:
  ///   - service: The scripted service.
  ///   - shotOutlet: Where copied pictures go and how files are shown.
  /// - Returns: The viewer, its log and screenshots read.
  /// - Throws: The failed requirement, when they never are.
  static func onShots(
    _ service: ScriptedService,
    shotOutlet: ShotOutlet = ShotOutlet(copy: { _, _ in true }, reveal: { _ in true })
  ) async throws -> ViewerController {
    service.shots = shots
    service.activityLog = log
    if service.shot.isEmpty { service.shot = png() }
    let viewer = try await ViewerController.watching(
      service,
      shotOutlet: shotOutlet
    )
    service.holdShots = true
    viewer.send(.show(.activity))
    try await settle("the screenshots were never read") {
      viewer.panes.shots.shots != nil && viewer.panes.activity.readAt != nil
    }
    return viewer
  }

  /// An outlet that hands each copied picture to `copied` and says it was copied.
  ///
  /// - Parameter copied: Receives each picture copied.
  /// - Returns: The outlet.
  static func copying(_ copied: @escaping @MainActor (Data) -> Void) -> ShotOutlet {
    ShotOutlet(
      copy: { png, _ in
        copied(png)
        return true
      },
      reveal: { _ in true }
    )
  }

  /// The picture sheet the window shows.
  ///
  /// - Parameter viewer: The viewer.
  /// - Returns: The sheet, or nil while none shows.
  static func sheet(_ viewer: ViewerController) -> ViewerPictureSheet? {
    guard case .activity(let activity) = viewer.state.pane else { return nil }
    return activity.sheet
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

  @Test("Activity keeps a read failure after an empty reading and clears it only after a successful reading")
  func activityReadFailuresPersist() async {
    let service = ScriptedService()
    let viewer = ViewerController.scripted(service)
    defer { viewer.close() }
    let model = viewer.panesModel
    let generation = model.work.generation
    await model.readActivity(generation)
    #expect(viewer.panes.activity.log == ActivityLog())
    service.fail.insert("activity")
    await model.readActivity(generation)
    let failure = viewer.panes.activity.failure
    #expect(failure != nil)
    #expect(ViewerPresenter.activity(viewer.snapshot).empty?.headline == ViewerWords.activityFailed)
    await model.readActivity(generation)
    #expect(viewer.panes.activity.failure == failure)
    service.fail.remove("activity")
    await model.readActivity(generation)
    #expect(viewer.panes.activity.failure == nil)
    #expect(ViewerPresenter.activity(viewer.snapshot).empty?.headline == ViewerWords.noActivity)
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
    let stale = viewer.panesModel.thumbnails.work.loading[Self.newer]
    try await settle("the first thumbnail was never asked for") { service.heldShots.count == 1 }
    viewer.send(.show(.screen))
    viewer.send(.show(.activity))
    service.shot = png(width: 2, height: 2)
    viewer.send(.thumbnailNeeded(Self.newer))
    let fresh = viewer.panesModel.thumbnails.work.loading[Self.newer]
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

  @Test("leaving Activity for Apps lets the thumbnail queue go, so a reply starts no queued request")
  func appsLetsTheThumbnailsGo() async throws {
    let service = ScriptedService()
    let viewer = try await Self.onShots(service)
    for name in ["1-capture.png", "2-capture.png", "3-capture.png"] { viewer.send(.thumbnailNeeded(name)) }
    try await settle("two thumbnails were never asked for") { service.heldShots.count == 2 }
    let first = viewer.panesModel.thumbnails.work.loading["1-capture.png"]
    viewer.send(.show(.apps))
    try await Self.answer(service, "1-capture.png", full: false)
    try await Self.ended(first, "the first thumbnail's reply never came back")
    #expect(service.shotCalls == 2, "the third thumbnail was never asked for")
    service.resumeAllShots()
    viewer.close()
  }

  @Test("pending thumbnails resume when Activity shows again")
  func hiddenThumbnailsResume() async throws {
    let service = ScriptedService()
    let viewer = try await Self.onShots(service)
    defer {
      viewer.close()
      service.resumeAllShots()
    }
    let queued = "queued.png"
    let removed = "removed.png"
    service.shots += [Screenshot(name: queued), Screenshot(name: removed)]
    await viewer.panesModel.readActivity(viewer.panesModel.work.generation)
    service.shot = png(width: 1, height: 1)
    for name in [Self.newer, Self.older, queued, removed] { viewer.send(.thumbnailNeeded(name)) }
    try await settle("two thumbnails were never asked for") { service.heldShots.count == 2 }
    let stale = Array(viewer.panesModel.thumbnails.work.loading.values)
    viewer.setVisible(false)
    viewer.setVisible(false)
    for task in stale { #expect(task.isCancelled) }
    #expect(viewer.panesModel.thumbnails.work.loading.isEmpty)
    service.shots.removeAll { $0.name == removed }
    await viewer.panesModel.readActivity(viewer.panesModel.work.generation)
    service.shot = png(width: 2, height: 2)
    viewer.setVisible(true)
    try await settle("the cancelled thumbnails were not requested again") { service.heldShots.count == 4 }
    let fresh = Array(viewer.panesModel.thumbnails.work.loading.values)
    for name in [Self.newer, Self.older] { try await Self.answer(service, name, full: false) }
    for task in stale { try await Self.ended(task, "a stale thumbnail never replied") }
    #expect(viewer.panes.thumbnails[Self.newer] == nil)
    #expect(viewer.panes.thumbnails[Self.older] == nil)
    #expect(viewer.panesModel.thumbnails.work.loading.count == 2)
    #expect(service.heldShots.count == 4, "late replies do not start the queued thumbnail")
    for name in [Self.newer, Self.older] { try await Self.answer(service, name, full: false) }
    for task in fresh { try await Self.ended(task, "a resumed thumbnail never arrived") }
    try await settle("the queued thumbnail was never requested") { service.heldShots.count == 5 }
    let last = viewer.panesModel.thumbnails.work.loading[queued]
    try await Self.answer(service, queued, full: false)
    try await Self.ended(last, "the queued thumbnail never arrived")
    for name in [Self.newer, Self.older, queued] {
      #expect(viewer.panes.thumbnails[name]?.image.width == 2)
    }
    #expect(viewer.panesModel.thumbnails.work.wanted.isEmpty)
    #expect(!service.heldShots.contains { $0.name == removed })
    viewer.setVisible(false)
    viewer.setVisible(true)
    #expect(viewer.panesModel.thumbnails.work.loading.isEmpty, "cached thumbnails need no new requests")
  }

  @Test("a screenshot opens in the picture sheet over Activity, read as taken, and steps through its run's screenshots")
  func aScreenshotOpensInTheSheet() async throws {
    let service = ScriptedService()
    let viewer = try await Self.onShots(service)
    defer {
      viewer.close()
      service.resumeAllShots()
    }
    viewer.send(.openShot(Self.newer))
    #expect(viewer.state.pane.place == .activity)
    let opened = try #require(Self.sheet(viewer))
    #expect(opened.id == Self.newer)
    #expect(opened.position == "2 of 2")
    #expect(opened.canGoBack)
    #expect(!opened.canGoForward)
    let load = try #require(viewer.panesModel.pictures.loads[Self.newer])
    try await Self.answer(service, Self.newer, full: true)
    try await Self.ended(load, "the picture never arrived")
    #expect(viewer.panes.activity.pictures[Self.newer] != nil)
    viewer.send(.previousPicture)
    let older = try #require(Self.sheet(viewer))
    #expect(older.id == Self.older)
    #expect(older.position == "1 of 2")
    #expect(!older.canGoBack)
    #expect(viewer.panes.activity.pictures[Self.newer] == nil, "a picture no longer shown is let go")
    viewer.send(.previousPicture)
    #expect(Self.sheet(viewer)?.id == Self.older, "‹ goes no further than the run's oldest")
    viewer.send(.closePicture)
    #expect(Self.sheet(viewer) == nil)
    #expect(viewer.panesModel.pictures.loads.isEmpty)
  }

  @Test("a screenshot whose row leaves the log closes the sheet and lets its picture go")
  func aScreenshotLeavingTheLogClosesTheSheet() async throws {
    let service = ScriptedService()
    let viewer = try await Self.onShots(service)
    defer {
      viewer.close()
      service.resumeAllShots()
    }
    viewer.send(.openShot(Self.older))
    let load = try #require(viewer.panesModel.pictures.loads[Self.older])
    service.activityLog = ActivityLog(entries: Array(Self.log.entries.prefix(1)))
    await viewer.panesModel.readActivity(viewer.panesModel.work.generation)
    #expect(viewer.panes.activity.sheet == nil)
    #expect(Self.sheet(viewer) == nil)
    #expect(load.isCancelled)
    try await Self.answer(service, Self.older, full: true)
    try await Self.ended(load, "the closed picture never replied")
    #expect(viewer.panes.activity.pictures[Self.older] == nil)
  }

  @Test("an older Copy never lands over a newer one")
  func olderCopyNeverLands() async throws {
    let service = ScriptedService()
    service.shotData = [Self.newer: png(width: 1, height: 1), Self.older: png(width: 2, height: 2)]
    var copied: [Data] = []
    let viewer = try await Self.onShots(service, shotOutlet: Self.copying { copied.append($0) })
    defer {
      viewer.close()
      service.resumeAllShots()
    }
    viewer.send(.openShot(Self.newer))
    viewer.send(.copyPicture)
    let firstCopy = try #require(viewer.panesModel.pictures.copying)
    viewer.send(.previousPicture)
    viewer.send(.copyPicture)
    let secondCopy = try #require(viewer.panesModel.pictures.copying)
    #expect(firstCopy.isCancelled)
    try await settle("the second Copy was never asked for") {
      service.heldShots.filter { $0.name == Self.older && $0.full }.count == 2
    }
    service.resumeAllShots()
    try await Self.ended(secondCopy, "the second Copy's reply never came back")
    try await Self.ended(firstCopy, "the first Copy's reply never came back")
    #expect(copied == [service.shotData[Self.older]])
  }

  @Test("Copy puts the picture on the clipboard though the screenshots' folder is not known")
  func copyNeedsNoFolder() async throws {
    let service = ScriptedService()
    service.shotData = [Self.newer: png(width: 1, height: 1), Self.older: png(width: 2, height: 2)]
    var copied: [Data] = []
    let viewer = try await Self.onShots(service, shotOutlet: Self.copying { copied.append($0) })
    defer {
      viewer.close()
      service.resumeAllShots()
    }
    viewer.panesModel.panes.shots.folder = nil
    viewer.send(.openShot(Self.newer))
    let sheet = try #require(ViewerPresenter.activity(viewer.snapshot).sheet)
    #expect(!sheet.canShowInFinder)
    viewer.send(.copyPicture)
    let copy = try #require(viewer.panesModel.pictures.copying)
    try await settle("the Copy was never asked for") {
      service.heldShots.filter { $0.name == Self.newer && $0.full }.count == 2
    }
    service.resumeAllShots()
    try await Self.ended(copy, "the Copy's reply never came back")
    #expect(copied == [service.shotData[Self.newer]])
  }

  @Test("closing the sheet cancels a Copy still being read, so it never lands over what is copied next")
  func closingTheSheetCancelsItsCopy() async throws {
    let service = ScriptedService()
    var copied: [Data] = []
    let viewer = try await Self.onShots(service, shotOutlet: Self.copying { copied.append($0) })
    defer {
      viewer.close()
      service.resumeAllShots()
    }
    viewer.send(.openShot(Self.newer))
    viewer.send(.copyPicture)
    let copy = try #require(viewer.panesModel.pictures.copying)
    // The sheet reads the picture to show it, and Copy reads it again.
    try await settle("the Copy was never asked for") { service.heldShots.count == 2 }
    viewer.send(.closePicture)
    #expect(copy.isCancelled)
    #expect(viewer.panesModel.pictures.copying == nil)
    service.resumeAllShots()
    try await Self.ended(copy, "the cancelled Copy never replied")
    #expect(copied.isEmpty)
  }

  @Test("a delayed picture reply after close does not run the save outlet")
  func delayedSaveDoesNotRunAfterClose() async throws {
    let service = ScriptedService()
    service.shot = png()
    var saved = false
    let viewer = try await Self.onShots(
      service,
      shotOutlet: ShotOutlet(
        copy: { _, _ in true },
        reveal: { _ in true },
        save: { _, _ in
          saved = true
          return true
        }
      )
    )
    viewer.send(.openShot(Self.newer))
    viewer.panesModel.pictures.save {}
    let save = try #require(viewer.panesModel.pictures.copying)
    try await settle("the picture was never asked for") { service.heldShots.count == 2 }
    viewer.close()
    service.resumeAllShots()
    try await Self.ended(save, "the picture's reply never came back")
    #expect(!saved)
    #expect(save.isCancelled)
    #expect(viewer.panesModel.pictures.copying == nil)
  }

  @Test("the sheet's picture read while the window hides never lands, and is read again once it shows")
  func sheetReadStopsWhileHidden() async throws {
    let service = ScriptedService()
    let viewer = try await Self.onShots(service)
    defer {
      viewer.close()
      service.resumeAllShots()
    }
    viewer.send(.openShot(Self.newer))
    let hiddenLoad = try #require(viewer.panesModel.pictures.loads[Self.newer])
    viewer.setVisible(false)
    #expect(hiddenLoad.isCancelled)
    try await Self.answer(service, Self.newer, full: true)
    try await Self.ended(hiddenLoad, "the picture's reply never came back")
    #expect(viewer.panes.activity.pictures[Self.newer] == nil)
    viewer.setVisible(true)
    let shownLoad = try #require(viewer.panesModel.pictures.loads[Self.newer], "the picture is read again")
    try await Self.answer(service, Self.newer, full: true)
    try await Self.ended(shownLoad, "the picture read again never arrived")
    #expect(viewer.panes.activity.pictures[Self.newer] != nil)
  }

  @Test("a sheet's picture that could not be read is read again with the next reading of Activity")
  func failedSheetReadIsTriedAgain() async throws {
    let service = ScriptedService()
    service.shots = Self.shots
    service.activityLog = Self.log
    service.fail = ["screenshot"]
    let viewer = try await ViewerController.watching(service)
    defer { viewer.close() }
    viewer.send(.show(.activity))
    try await settle("Activity was never read") { viewer.panes.shots.shots != nil }
    viewer.send(.openShot(Self.newer))
    try await Self.ended(viewer.panesModel.pictures.loads[Self.newer], "the failed read never ended")
    #expect(viewer.panesModel.pictures.loads[Self.newer] == nil)
    service.fail = []
    service.shot = png()
    await viewer.panesModel.readActivity(viewer.panesModel.work.generation)
    let retry = try #require(viewer.panesModel.pictures.loads[Self.newer], "the next reading reads it again")
    try await Self.ended(retry, "the second read never ended")
    #expect(viewer.panes.activity.pictures[Self.newer] != nil)
  }

  @Test("Show in Finder shows only a listed screenshot, inside the folder the service names")
  func showInFinderRevealsTheListedFile() async throws {
    let service = ScriptedService()
    service.shotsFolder = URL(fileURLWithPath: "/Users/me/runs", isDirectory: true)
    var revealed: [URL] = []
    let viewer = try await Self.onShots(
      service,
      shotOutlet: ShotOutlet(
        copy: { _, _ in true },
        reveal: { file in
          revealed.append(file)
          return true
        }
      )
    )
    defer {
      viewer.close()
      service.resumeAllShots()
    }
    viewer.send(.openShot(Self.newer))
    #expect(Self.sheet(viewer)?.canShowInFinder == true)
    viewer.send(.showPictureInFinder)
    #expect(revealed.map(\.path) == ["/Users/me/runs/\(Self.newer)"])
    service.shotsFolder = nil
    await viewer.panesModel.readActivity(viewer.panesModel.work.generation)
    try #require(viewer.panes.shots.folder == nil, "a reading that names no folder leaves it unknown")
    #expect(Self.sheet(viewer)?.canShowInFinder == false)
    viewer.send(.showPictureInFinder)
    #expect(revealed.count == 1, "no file is shown while the folder is unknown")
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
}
