import AppKit
import AsyncSupportTesting
import AtlasService
import AtlasServiceTesting
import AtlasStatus
import AtlasUI
import CoreGraphics
import Foundation
import Observation
import SwiftUI
import Testing

@testable import AtlasApp

@Suite("Viewer redraws")
@MainActor
struct ViewerRedrawTests {
  /// A row the service started at a fixed moment, which it says was `age` seconds before the reading.
  ///
  /// - Parameters:
  ///   - id: The row's id.
  ///   - age: Its age at the reading, in seconds.
  ///   - operation: Its operation.
  ///   - capture: The capture a screenshot row took.
  /// - Returns: The row.
  static func row(_ id: String, age: Double, operation: String = "input", capture: String? = nil) -> ActivityLog.Entry {
    ActivityLog.Entry(
      id: id,
      says: "clicked Save in TextEdit",
      ageSeconds: age,
      startedAt: Date(timeIntervalSince1970: 1_000),
      operation: operation,
      capture: capture,
      verb: "click",
      app: "TextEdit"
    )
  }

  /// A picture `width` by `height` pixels.
  ///
  /// - Parameters:
  ///   - id: Its id.
  ///   - width: Its width.
  ///   - height: Its height.
  /// - Returns: The picture.
  /// - Throws: The failed requirement, when the image cannot be made.
  static func picture(_ id: String, width: Int = 4, height: Int = 3) throws -> DesktopPicture {
    let context = try #require(
      CGContext(
        data: nil,
        width: width,
        height: height,
        bitsPerComponent: 8,
        bytesPerRow: 0,
        space: CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue
      )
    )
    return DesktopPicture(id: id, image: try #require(context.makeImage()))
  }

  /// Shows `picture` as the newest live picture, as the frame service and the viewer's own reading do.
  ///
  /// - Parameters:
  ///   - picture: The picture.
  ///   - viewer: The viewer.
  ///   - service: The service whose display it shows.
  static func show(_ picture: DesktopPicture, in viewer: ViewerController, service: ScriptedService) {
    viewer.readingLoop.show(
      Frame(png: Data(), id: picture.id, revision: service.revision, display: service.display),
      decoded: DesktopPicture.Decoded(
        picture: picture,
        size: CGSize(width: picture.image.width, height: picture.image.height)
      ),
      ready: true
    )
  }

  /// Shows Activity, once its own first reading is taken in.
  ///
  /// - Parameter viewer: The viewer.
  /// - Throws: The failed requirement, when the reading never lands.
  static func showActivity(_ viewer: ViewerController) async throws {
    viewer.send(.show(.activity))
    try await settle("Activity was never read") { viewer.panes.activity.log != nil }
  }

  /// Whether `change` tells the window to draw again, as its view follows the state it draws.
  ///
  /// - Parameters:
  ///   - viewer: The viewer.
  ///   - change: The change.
  /// - Returns: Whether it does.
  static func presents(_ viewer: ViewerController, _ change: () async throws -> Void) async rethrows -> Bool {
    let presented = Presented()
    withObservationTracking {
      _ = viewer.drawnState()
    } onChange: {
      presented.again = true
    }
    try await change()
    return presented.again
  }

  @Test("a new live picture doesn't present the window again")
  func aNewLivePictureDoesntPresentTheWindowAgain() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    defer { viewer.close() }
    let laidOut = try #require(viewer.reading.picture)
    let newest = try Self.picture("newest")
    let again = try await Self.presents(viewer) {
      for index in 0..<5 { Self.show(try Self.picture("frame \(index)"), in: viewer, service: service) }
      Self.show(newest, in: viewer, service: service)
    }
    #expect(!again)
    #expect(viewer.readingLoop.feed.picture == newest, "the live slots draw the newest")
    #expect(viewer.reading.frame?.id == newest.id, "input is aimed at the newest")
    #expect(viewer.reading.picture == laidOut, "the window keeps the picture it is laid out with")
  }

  @Test("a live picture of a new size presents the window once")
  func aLivePictureOfANewSizePresentsTheWindowOnce() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    defer { viewer.close() }
    let wider = try Self.picture("wider", width: 8, height: 3)
    let resized = try await Self.presents(viewer) { Self.show(wider, in: viewer, service: service) }
    #expect(resized)
    #expect(viewer.drawnState().screen.picture == wider)
    let next = try Self.picture("next", width: 8, height: 3)
    let again = try await Self.presents(viewer) { Self.show(next, in: viewer, service: service) }
    #expect(!again)
  }

  @Test("while Apps shows, every live picture presents the window, as its cards are cut from each")
  func whileAppsShowsEveryLivePicturePresentsTheWindow() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    defer { viewer.close() }
    Self.show(try Self.picture("before Apps"), in: viewer, service: service)
    viewer.send(.show(.apps))
    #expect(viewer.reading.picture?.id == viewer.reading.frame?.id, "Apps starts from the newest picture")
    let next = try Self.picture("next")
    let presented = try await Self.presents(viewer) { Self.show(next, in: viewer, service: service) }
    #expect(presented)
    #expect(viewer.reading.picture == next)
  }

  @Test("a clock tick doesn't present the window again")
  func aClockTickDoesntPresentTheWindowAgain() async throws {
    let service = ScriptedService()
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(service, clock: clock)
    defer { viewer.close() }
    viewer.readingLoop.readClock()
    let again = try await Self.presents(viewer) {
      clock.advance(by: .seconds(1))
      viewer.readingLoop.readClock()
    }
    #expect(!again)
  }

  @Test("a clock tick that leaves the picture too old to take control over presents the window")
  func aClockTickThatLeavesThePictureTooOldPresentsTheWindow() async throws {
    let service = ScriptedService()
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(service, clock: clock)
    defer { viewer.close() }
    viewer.readingLoop.readClock()
    #expect(viewer.drawnState().footer.controls.control?.isEnabled == true)
    // No reading answers from here on, so no newer picture arrives.
    service.hold = ["status", "frame"]
    defer {
      service.resume("status")
      service.resume("frame")
    }
    let stale = try await Self.presents(viewer) {
      clock.advance(by: .seconds(Freshness.stale))
      viewer.readingLoop.readClock()
    }
    #expect(stale)
    #expect(viewer.drawnState().footer.controls.control?.isEnabled != true, "control is offered over a stale picture")
    let fresh = try await Self.presents(viewer) { Self.show(try Self.picture("fresh"), in: viewer, service: service) }
    #expect(fresh, "a fresh picture of the same size never offered control again")
    #expect(viewer.drawnState().footer.controls.control?.isEnabled == true)
  }

  @Test("an unchanged status reading changes nothing the window shows")
  func anUnchangedStatusReadingChangesNothingTheWindowShows() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    defer { viewer.close() }
    await viewer.readingLoop.readStatus()
    let again = try await Self.presents(viewer) {
      for _ in 0..<3 { await viewer.readingLoop.readStatus() }
    }
    #expect(!again)
  }

  @Test("an unchanged sessions reading changes nothing the window shows")
  func anUnchangedSessionsReadingChangesNothingTheWindowShows() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    defer { viewer.close() }
    let model = viewer.panesModel
    await model.readSessions(model.work.sessionsGeneration)
    let again = try await Self.presents(viewer) {
      await model.readSessions(model.work.sessionsGeneration)
    }
    #expect(!again)
  }

  @Test("an unchanged activity reading changes nothing the window shows, and lists no screenshots")
  func anUnchangedActivityReadingChangesNothingTheWindowShows() async throws {
    let service = ScriptedService()
    service.activityLog = ActivityLog(entries: [
      Self.row("a", age: 2), Self.row("shot", age: 4, operation: "screenshot", capture: "shot.png"),
    ])
    service.shots = [Screenshot(name: "shot.png", takenAt: Date(timeIntervalSince1970: 996))]
    var listed = 0
    service.holdWhen = { route in
      if route == "screenshots" { listed += 1 }
      return false
    }
    let viewer = try await ViewerController.watching(service)
    defer { viewer.close() }
    try await Self.showActivity(viewer)
    #expect(listed == 1)
    // A later reading of the same rows: each is two seconds older, its start the same.
    service.activityLog = ActivityLog(entries: [
      Self.row("a", age: 4), Self.row("shot", age: 6, operation: "screenshot", capture: "shot.png"),
    ])
    let again = try await Self.presents(viewer) {
      await viewer.panesModel.readActivity(viewer.panesModel.work.generation)
    }
    #expect(!again)
    #expect(listed == 1, "the screenshots were listed again for the same log")
  }

  @Test("a changed activity reading shows its new rows and lists the screenshots again")
  func aChangedActivityReadingShowsItsNewRows() async throws {
    let service = ScriptedService()
    service.activityLog = ActivityLog(entries: [Self.row("shot", age: 4, operation: "screenshot", capture: "s.png")])
    service.shots = [Screenshot(name: "s.png", takenAt: Date(timeIntervalSince1970: 996))]
    var listed = 0
    service.holdWhen = { route in
      if route == "screenshots" { listed += 1 }
      return false
    }
    let viewer = try await ViewerController.watching(service)
    defer { viewer.close() }
    try await Self.showActivity(viewer)
    service.activityLog.entries.insert(Self.row("new", age: 1), at: 0)
    let again = try await Self.presents(viewer) {
      await viewer.panesModel.readActivity(viewer.panesModel.work.generation)
    }
    #expect(again)
    #expect(viewer.panes.activity.log?.entries.map(\.id) == ["new", "shot"])
    #expect(listed == 2)
  }

  @Test("a screenshot row whose capture wasn't listed yet lists the screenshots again with the same log")
  func aScreenshotRowWhoseCaptureWasntListedYetListsTheScreenshotsAgain() async throws {
    let service = ScriptedService()
    service.activityLog = ActivityLog(entries: [Self.row("shot", age: 4, operation: "screenshot", capture: "s.png")])
    var listed = 0
    service.holdWhen = { route in
      if route == "screenshots" { listed += 1 }
      return false
    }
    let viewer = try await ViewerController.watching(service)
    defer { viewer.close() }
    try await Self.showActivity(viewer)
    service.shots = [Screenshot(name: "s.png", takenAt: Date(timeIntervalSince1970: 996))]
    await viewer.panesModel.readActivity(viewer.panesModel.work.generation)
    #expect(listed == 2)
    #expect(viewer.panes.shots.shots?.map(\.name) == ["s.png"])
    let again = try await Self.presents(viewer) {
      await viewer.panesModel.readActivity(viewer.panesModel.work.generation)
    }
    #expect(!again)
    #expect(listed == 2, "once every capture is found, the same log lists nothing")
  }

  @Test(
    "the same log with a capture still missing, or screenshots that can't be listed, lists them again but changes nothing"
  )
  func theSameLogWithACaptureStillMissingListsAgainButChangesNothing() async throws {
    let service = ScriptedService()
    service.activityLog = ActivityLog(entries: [Self.row("shot", age: 4, operation: "screenshot", capture: "s.png")])
    var listed = 0
    service.holdWhen = { route in
      if route == "screenshots" { listed += 1 }
      return false
    }
    let viewer = try await ViewerController.watching(service)
    defer { viewer.close() }
    try await Self.showActivity(viewer)
    let missing = try await Self.presents(viewer) {
      for _ in 0..<2 { await viewer.panesModel.readActivity(viewer.panesModel.work.generation) }
    }
    #expect(!missing)
    #expect(listed == 3, "a missing capture is looked for again with each reading")
    service.fail = ["screenshots"]
    let failing = try await Self.presents(viewer) {
      for _ in 0..<2 { await viewer.panesModel.readActivity(viewer.panesModel.work.generation) }
    }
    #expect(!failing)
    #expect(listed == 5, "a failed listing is tried again with each reading")
  }

  @Test("a lost picture keeps the newest picture behind its words")
  func aLostPictureKeepsTheNewestPictureBehindItsWords() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    defer { viewer.close() }
    let newest = try Self.picture("newest")
    Self.show(newest, in: viewer, service: service)
    viewer.readingLoop.lost(ServiceError("The helper went away"))
    #expect(viewer.drawnState().screen.empty?.lastPicture == newest)
  }

  @Test("copy picture copies the newest live picture")
  func copyPictureCopiesTheNewestLivePicture() async throws {
    let service = ScriptedService()
    var copied: [CGImage] = []
    let viewer = try await ViewerController.watching(
      service,
      pictures: PictureOutlet(copy: { copied.append($0) }, save: { _ in nil })
    )
    defer { viewer.close() }
    let newest = try Self.picture("newest")
    Self.show(newest, in: viewer, service: service)
    viewer.readingLoop.copyPicture()
    #expect(copied.count == 1)
    #expect(copied.first === newest.image)
  }

  @Test("the live slot draws each new picture while the window draws nothing else again")
  func theLiveSlotDrawsEachNewPicture() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    defer { viewer.close() }
    viewer.send(.show(.activity))
    let window = NSWindow(
      contentRect: NSRect(origin: .zero, size: PreviewStates.viewerSize),
      styleMask: [.borderless],
      backing: .buffered,
      defer: false
    )
    window.isReleasedWhenClosed = false
    defer { window.close() }
    var bodies = 0
    let feed = viewer.readingLoop.feed
    let hosting = NSHostingView(
      rootView: ViewerRedrawRoot(counted: { bodies += 1 }) {
        ViewerView(
          state: viewer.drawnState(),
          send: { viewer.send($0) },
          input: { EmptyView() },
          livePicture: { LayerPictureView(live: $0, feed: feed) }
        )
      }
    )
    hosting.frame = NSRect(origin: .zero, size: PreviewStates.viewerSize)
    window.contentView = hosting
    hosting.layoutSubtreeIfNeeded()
    let before = bodies
    let newest = try Self.picture("newest")
    for index in 0..<3 {
      Self.show(try Self.picture("frame \(index)"), in: viewer, service: service)
      hosting.layoutSubtreeIfNeeded()
    }
    Self.show(newest, in: viewer, service: service)
    hosting.layoutSubtreeIfNeeded()
    #expect(bodies == before, "the window drew itself again for a picture")
    let slots = LayerPictureTests.pictureViews(in: hosting)
    #expect(!slots.isEmpty, "the small screen draws no picture")
    #expect(slots.allSatisfy { ($0.layer?.contents as AnyObject?) === newest.image })
  }
}
