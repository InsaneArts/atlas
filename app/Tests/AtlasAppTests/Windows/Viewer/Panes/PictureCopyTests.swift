import AppKit
import AsyncSupportTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import ImageIO
import Testing
import UniformTypeIdentifiers

@testable import AtlasApp

@MainActor
@Suite("Picture copy", .serialized)
struct PictureCopyTests {
  @Test("Copy puts a screenshot on the clipboard as PNG, TIFF and its file, then says Copied for a moment")
  func copyingAScreenshot() async throws {
    let pasteboard = Self.pasteboard()
    defer { pasteboard.releaseGlobally() }
    let folder = try Self.folder()
    defer {
      try? FileManager.default
        .removeItem(at: folder)
    }
    let service = ScriptedService()
    let shot = png(width: 4, height: 3)
    service.shotData = [ViewerPanesModelTests.newer: shot]
    service.shots = ViewerPanesModelTests.shots
    service.activityLog = ViewerPanesModelTests.log
    let file = folder.appendingPathComponent(ViewerPanesModelTests.newer)
    try shot.write(to: file)
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(
      service,
      clock: clock,
      shotOutlet: .live(.shared, pasteboard: pasteboard)
    )
    defer { viewer.close() }
    viewer.send(.show(.activity))
    try await settle("the screenshots were never read") { viewer.panes.shots.shots != nil }
    viewer.panesModel.panes.shots.folder = folder
    viewer.send(.openShot(ViewerPanesModelTests.newer))
    viewer.send(.copyPicture)
    try await settle("Copy never said it copied") { Self.notice(viewer) == .copied }
    #expect(pasteboard.data(forType: .png) == shot)
    let tiff = try #require(pasteboard.data(forType: .tiff), "apps that take only TIFF can paste it")
    #expect(NSImage(data: tiff)?.size == CGSize(width: 4, height: 3))
    let pasted = try #require(pasteboard.string(forType: .fileURL).flatMap(URL.init(string:)))
    #expect(pasted.standardizedFileURL == file.standardizedFileURL, "the Finder and apps that take files can paste it")
    try await clock.untilSleeping(for: ViewerPictureViews.noticeHold, "Copied never waited to go")
    clock.advance(by: ViewerPictureViews.noticeHold)
    try await settle("Copied never went back to Copy") { Self.notice(viewer) == nil }
  }

  @Test("Copy puts a step's picture, kept as a JPEG, on the clipboard as a PNG")
  func copyingAStepsPicture() async throws {
    let pasteboard = Self.pasteboard()
    defer { pasteboard.releaseGlobally() }
    let service = ScriptedService()
    var copied: [Data] = []
    let outlet = ShotOutlet(
      copy: { png, file in
        copied.append(png)
        return ShotOutlet.copy(png, file: file, to: pasteboard)
      },
      reveal: { _ in true }
    )
    let viewer = try await ViewerPictureViewsTests.onActivity(service, shotOutlet: outlet)
    defer { viewer.close() }
    let names = ViewerPictureViewsTests.names
    service.shotData[names.second] = try Self.jpeg(width: 4, height: 3)
    viewer.send(.openAction("c2"))
    viewer.send(.copyPicture)
    try await settle("Copy never said it copied") { Self.notice(viewer) == .copied }
    let pasted = try #require(pasteboard.data(forType: .png))
    let source = try #require(CGImageSourceCreateWithData(pasted as CFData, nil))
    #expect(CGImageSourceGetType(source) as String? == UTType.png.identifier)
    #expect(NSImage(data: pasted)?.size == CGSize(width: 4, height: 3))
    #expect(copied.count == 1)
  }

  @Test("Copy that cannot read its picture says so in the sheet, and leaves the clipboard as it was")
  func aFailedCopySaysSo() async throws {
    let pasteboard = Self.pasteboard()
    defer { pasteboard.releaseGlobally() }
    pasteboard.clearContents()
    pasteboard.setString("kept", forType: .string)
    let service = ScriptedService()
    let viewer = try await ViewerPictureViewsTests.onActivity(
      service,
      shotOutlet: .live(.shared, pasteboard: pasteboard)
    )
    defer { viewer.close() }
    viewer.send(.openAction("c2"))
    service.fail.insert("screenshot")
    viewer.send(.copyPicture)
    try await settle("Copy never said it failed") { Self.notice(viewer) == .copyFailed }
    #expect(pasteboard.string(forType: .string) == "kept")
  }

  @Test("Show in Finder whose file is not there says so in the sheet")
  func aMissingFileSaysSo() async throws {
    let service = ScriptedService()
    var revealed: [URL] = []
    let viewer = try await ViewerPanesModelTests.onShots(
      service,
      shotOutlet: ShotOutlet(
        copy: { _, _ in true },
        reveal: { file in
          revealed.append(file)
          return false
        }
      )
    )
    defer {
      viewer.close()
      service.resumeAllShots()
    }
    viewer.panesModel.panes.shots.folder = FileManager.default.temporaryDirectory
      .appendingPathComponent("atlas-missing-\(UUID().uuidString)", isDirectory: true)
    viewer.send(.openShot(ViewerPanesModelTests.newer))
    viewer.send(.showPictureInFinder)
    #expect(revealed.count == 1)
    #expect(Self.notice(viewer) == .fileMissing)
  }

  /// What the sheet's footer says now.
  ///
  /// - Parameter viewer: The window.
  /// - Returns: The notice, or nil.
  static func notice(_ viewer: ViewerController) -> ViewerSheetNotice? {
    ViewerPresenter.activity(viewer.snapshot).sheet?.notice
  }

  /// A pasteboard of the test's own, so the person's clipboard is never touched.
  ///
  /// - Returns: The pasteboard.
  static func pasteboard() -> NSPasteboard {
    NSPasteboard(name: NSPasteboard.Name("atlas-copy-test-\(UUID().uuidString)"))
  }

  /// A folder of the test's own.
  ///
  /// - Returns: The folder.
  /// - Throws: When it cannot be made.
  static func folder() throws -> URL {
    let folder = FileManager.default.temporaryDirectory
      .appendingPathComponent("atlas-copy-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default
      .createDirectory(at: folder, withIntermediateDirectories: true)
    return folder
  }

  /// A JPEG of `width` by `height` pixels, as a step's picture is kept.
  ///
  /// - Parameters:
  ///   - width: Its width.
  ///   - height: Its height.
  /// - Returns: The JPEG.
  /// - Throws: When it cannot be made.
  static func jpeg(width: Int, height: Int) throws -> Data {
    let source = try #require(CGImageSourceCreateWithData(png(width: width, height: height) as CFData, nil))
    let image = try #require(CGImageSourceCreateImageAtIndex(source, 0, nil))
    let data = NSMutableData()
    let type = UTType.jpeg.identifier as CFString
    let destination = try #require(CGImageDestinationCreateWithData(data as CFMutableData, type, 1, nil))
    CGImageDestinationAddImage(destination, image, nil)
    try #require(CGImageDestinationFinalize(destination))
    return data as Data
  }
}
