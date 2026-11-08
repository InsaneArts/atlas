import AppKit
import AsyncSupportTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import SwiftUI
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Viewer window sheet", .serialized, AppKitState())
@MainActor
struct ViewerWindowSheetTests {
  @Test("in the real window the sheet shows the picture as taken, and Copy reads Copied, then Copy again")
  func copyReadsCopiedInTheWindow() async throws {
    _ = NSApplication.shared
    let service = ScriptedService()
    service.shotData = [ViewerPanesModelTests.newer: Self.stripes]
    service.shots = ViewerPanesModelTests.shots
    service.activityLog = ViewerPanesModelTests.log
    let clock = ManualClock()
    let pasteboard = NSPasteboard(name: NSPasteboard.Name("atlas-window-copy-\(UUID().uuidString)"))
    defer { pasteboard.releaseGlobally() }
    let viewer = try await ViewerController.watching(
      service,
      clock: clock,
      shotOutlet: .live(.shared, pasteboard: pasteboard)
    )
    defer { viewer.close() }
    viewer.send(.show(.activity))
    try await settle("the screenshots were never read") { viewer.panes.shots.shots != nil }
    let window = ViewerWindow(controller: viewer) { _ in }
    defer {
      window.close()
      NSWindow.removeFrame(usingName: ViewerWindow.autosaveName)
    }
    window.setContentSize(PreviewStates.viewerSize)
    viewer.send(.openShot(ViewerPanesModelTests.newer))
    try await settle("the picture was never read as taken") {
      viewer.panes.activity.pictures[ViewerPanesModelTests.newer] != nil
    }
    // The sheet is handed the picture at its own size, never the list's small one.
    let shown = try #require(ViewerPresenter.activity(viewer.snapshot).sheet?.picture)
    #expect(shown.image.width == Self.stripesSize.width)
    try await Self.draw(window)
    let well = try Self.pixels(window, in: Self.wellMiddle)
    #expect(Self.span(well) > Self.sharp, "the well draws the picture's stripes, not a blur of them")
    let copy = try Self.pixels(window, in: Self.footerTrailing)
    // Copy's own action, as its button sends it.
    viewer.send(.copyPicture)
    try await settle("Copy never said it copied") {
      ViewerPresenter.activity(viewer.snapshot).sheet?.notice == .copied
    }
    try await Self.draw(window)
    #expect(try Self.pixels(window, in: Self.footerTrailing) != copy, "the footer reads Copied")
    try await clock.untilSleeping(for: ViewerPictureViews.noticeHold, "Copied never waited to go")
    clock.advance(by: ViewerPictureViews.noticeHold)
    try await settle("Copied never went") { ViewerPresenter.activity(viewer.snapshot).sheet?.notice == nil }
    try await Self.draw(window)
    #expect(try Self.pixels(window, in: Self.footerTrailing) == copy, "the footer reads Copy again")
  }

  /// The window's content view, as the viewer window hosts it.
  typealias Hosting = NSHostingView<
    ShellRoot<
      ModifiedContent<
        ViewerView<DesktopInput, LayerPictureView>,
        _EnvironmentKeyWritingModifier<FocusedFieldReport?>
      >
    >
  >

  /// Lays the window out and draws it until its animations have run their course.
  ///
  /// - Parameter window: The window.
  /// - Throws: When the window does not host the viewer.
  static func draw(_ window: NSWindow) async throws {
    let hosting = try #require(window.contentView as? Hosting)
    for _ in 0..<turns {
      await afterQueuedWork()
      hosting.layoutSubtreeIfNeeded()
      hosting._renderForTest(interval: frame)
    }
  }

  /// The window's pixels in `rect`, in its points from the top left.
  ///
  /// - Parameters:
  ///   - window: The window.
  ///   - rect: The part.
  /// - Returns: The part's brightness, pixel by pixel.
  /// - Throws: When nothing was drawn.
  static func pixels(_ window: NSWindow, in rect: CGRect) throws -> [CGFloat] {
    let view = try #require(window.contentView)
    let drawn =
      view.isFlipped
      ? rect : CGRect(x: rect.minX, y: view.bounds.height - rect.maxY, width: rect.width, height: rect.height)
    let bitmap = try #require(view.bitmapImageRepForCachingDisplay(in: drawn))
    view.cacheDisplay(in: drawn, to: bitmap)
    return (0..<bitmap.pixelsHigh)
      .flatMap { y in
        (0..<bitmap.pixelsWide)
          .map { x in
            let colour = bitmap.colorAt(x: x, y: y)?
              .usingColorSpace(.sRGB)
            return colour?.brightnessComponent ?? 0
          }
      }
  }

  /// How far apart the darkest and brightest of `pixels` are.
  ///
  /// - Parameter pixels: The pixels.
  /// - Returns: The span.
  static func span(_ pixels: [CGFloat]) -> CGFloat {
    (pixels.max() ?? 0) - (pixels.min() ?? 0)
  }

  /// A screenshot of fine black and white stripes, 3 pixels each, which a blur turns grey.
  static let stripes: Data = {
    let size = stripesSize
    let context = CGContext(
      data: nil,
      width: size.width,
      height: size.height,
      bitsPerComponent: 8,
      bytesPerRow: 0,
      space: CGColorSpaceCreateDeviceRGB(),
      bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    )
    context?.setFillColor(CGColor(red: 1, green: 1, blue: 1, alpha: 1))
    context?.fill(CGRect(x: 0, y: 0, width: size.width, height: size.height))
    context?.setFillColor(CGColor(red: 0, green: 0, blue: 0, alpha: 1))
    for y in stride(from: 0, to: size.height, by: 6) {
      context?.fill(CGRect(x: 0, y: y, width: size.width, height: 3))
    }
    guard let image = context?.makeImage() else { return Data() }
    return NSBitmapImageRep(cgImage: image)
      .representation(using: .png, properties: [:]) ?? Data()
  }()

  /// 1600 × 1000: the screenshot's own size.
  static let stripesSize = (width: 1600, height: 1000)
  /// A strip down the middle of the sheet's well.
  static let wellMiddle = CGRect(x: 718, y: 250, width: 4, height: 100)
  /// The sheet footer's trailing half, where Copy stands.
  static let footerTrailing = CGRect(x: 760, y: 615, width: 220, height: 40)
  /// 0.8: how far apart the stripes' darkest and brightest pixels stand while drawn sharp.
  static let sharp: CGFloat = 0.8
  /// 40: frames enough for any of the sheet's animations.
  static let turns = 40
  /// 1/20 s: how far each frame moves the animations on.
  static let frame: TimeInterval = 0.05
}
