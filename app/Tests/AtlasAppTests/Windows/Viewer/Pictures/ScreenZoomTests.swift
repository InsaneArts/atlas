import AppKit
import AtlasUI
import SwiftUI
import Testing

@testable import AtlasApp

@Suite("ScreenZoom")
@MainActor
struct ScreenZoomTests {
  @Test("the small screen and the stage share one geometry id")
  func theSmallScreenAndTheStageShareOneGeometryID() throws {
    let leaving = try ZoomHost(.screen)
    // On the Screen, the stage's picture fills the middle of the window.
    #expect(try leaving.colour(at: ZoomHost.middle) == .stage)
    // Activity's first frame draws the small screen where the stage's picture was, whole over it: one picture, about
    // to shrink into the corner. Two pictures would leave the stage's own in the middle as it fades.
    try leaving.show(.activity)
    #expect(try leaving.colour(at: ZoomHost.middle) == .corner)

    let coming = try ZoomHost(.activity)
    // Over Activity, the small screen sits in its corner, drawn through the window's live slot (which paints its
    // picture blue here), and Activity has the middle.
    #expect(try coming.colour(at: ZoomHost.middle) == nil)
    #expect(try coming.colour(at: ZoomHost.corner) == .corner)
    // Back on the Screen, the stage's picture starts from the corner, under the small screen it grows out of, and
    // never in the middle, where it would stand apart from it.
    try coming.show(.screen)
    #expect(try coming.colour(at: ZoomHost.middle) == nil)
    #expect(try coming.colour(at: ZoomHost.corner) == .corner)
  }
}

/// The main window's content in an offscreen window, its live slot painting the stage's picture red and the corner's
/// blue, so a frame read straight after a switch shows where each picture stands as the switch begins.
@MainActor
private final class ZoomHost {
  /// A point in the middle of the stage, clear of the corner.
  static let middle = CGPoint(x: PreviewStates.viewerSize.width / 3, y: PreviewStates.viewerSize.height / 2)
  /// The middle of the small screen in its corner: 26 from the window's right and bottom edges, 248 by 155.
  static let corner = CGPoint(x: PreviewStates.viewerSize.width - 150, y: PreviewStates.viewerSize.height - 103)

  /// The window.
  private let window: NSWindow
  /// The hosted content.
  private let hosting: NSHostingView<ZoomStage>

  /// Hosts `tab`, laid out once, with nothing to move from.
  ///
  /// - Parameter tab: The tab shown first.
  /// - Throws: When a picture cannot be made.
  init(_ tab: ViewerTab) throws {
    window = NSWindow(
      contentRect: NSRect(origin: .zero, size: PreviewStates.viewerSize),
      styleMask: [.borderless],
      backing: .buffered,
      defer: false
    )
    window.isReleasedWhenClosed = false
    hosting = NSHostingView(rootView: try ZoomStage(tab))
    hosting.frame = NSRect(origin: .zero, size: PreviewStates.viewerSize)
    window.contentView = hosting
    hosting.layoutSubtreeIfNeeded()
  }

  /// Shows `tab` and lays the window out once: a switch's first frame.
  ///
  /// - Parameter tab: The tab.
  /// - Throws: When a picture cannot be made.
  func show(_ tab: ViewerTab) throws {
    hosting.rootView = try ZoomStage(tab)
    hosting.layoutSubtreeIfNeeded()
  }

  /// Which picture is drawn at `point`: the stage's, the corner's, or neither.
  ///
  /// - Parameter point: The point, in the window's points from its top left.
  /// - Returns: The picture's colour, or nil.
  /// - Throws: When nothing was drawn.
  func colour(at point: CGPoint) throws -> PictureColour? {
    let bitmap = try #require(hosting.bitmapImageRepForCachingDisplay(in: hosting.bounds))
    hosting.cacheDisplay(in: hosting.bounds, to: bitmap)
    let scale = CGFloat(bitmap.pixelsWide) / hosting.bounds.width
    let colour = try #require(
      bitmap.colorAt(x: Int(point.x * scale), y: Int(point.y * scale))?
        .usingColorSpace(.sRGB)
    )
    return PictureColour.allCases
      .first { $0.matches(colour) }
  }
}

/// The main window's content at its size, on the Screen or on Activity with the small screen in its corner. The stage
/// and the corner read one picture in the window; here each has its own, so the slot can paint them apart.
private struct ZoomStage: View {
  /// What the window shows.
  let state: ViewerState

  /// The content on `tab`.
  ///
  /// - Parameter tab: The Screen, or Activity.
  /// - Throws: When a picture cannot be made.
  init(_ tab: ViewerTab) throws {
    var state = tab == .screen ? PreviewStates.viewerWorking : PreviewStates.viewerActivity
    state.screen.picture = try LayerPictureTests.picture(PictureColour.stage.rawValue)
    state.mini =
      tab.keepsScreenInCorner
      ? MiniScreenState(picture: try LayerPictureTests.picture(PictureColour.corner.rawValue)) : nil
    self.state = state
  }

  var body: some View {
    ViewerView(state: state, send: { _ in }, newest: { "" }, input: { EmptyView() }) { live in
      (PictureColour(rawValue: live.picture.id) ?? .stage).color
    }
    .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
  }
}

/// The colour the test's slot paints each picture.
private enum PictureColour: String, CaseIterable {
  /// The stage's picture, red.
  case stage
  /// The corner's picture, blue.
  case corner

  /// The colour.
  var color: Color {
    switch self {
    case .stage: Color(red: 1, green: 0, blue: 0)
    case .corner: Color(red: 0, green: 0, blue: 1)
    }
  }

  /// Whether `colour` is this one, drawn whole or nearly.
  ///
  /// - Parameter colour: A drawn colour.
  /// - Returns: Whether it matches.
  func matches(_ colour: NSColor) -> Bool {
    switch self {
    case .stage: colour.redComponent > 0.8 && colour.blueComponent < 0.2
    case .corner: colour.blueComponent > 0.8 && colour.redComponent < 0.2
    }
  }
}
