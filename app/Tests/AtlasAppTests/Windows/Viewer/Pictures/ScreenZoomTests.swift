import AppKit
import AtlasUI
import SwiftUI
import Testing

@testable import AtlasApp

@Suite("ScreenZoom", .serialized, AppKitState())
@MainActor
struct ScreenZoomTests {
  @Test("the small screen and the stage share one geometry id")
  func theSmallScreenAndTheStageShareOneGeometryID() throws {
    // On the Screen, the stage's picture fills the middle of the window.
    #expect(try ViewerHost(try Self.state(.screen)).colour(at: Self.middle) == .stage)
    // Activity's first frame draws the small screen where the stage's picture was, whole over it: one picture, about
    // to shrink into the corner. Two pictures would leave the stage's own in the middle as it fades.
    let leaving = try ViewerHost.firstFrame(
      from: try Self.state(.screen),
      to: try Self.state(.activity),
      at: [Self.middle]
    )
    #expect(leaving == [.corner])
    // Over Activity, the small screen sits in its corner, drawn through the window's live slot (which paints its
    // picture blue here), and Activity has the middle.
    let settled = try ViewerHost(try Self.state(.activity))
      .colours(at: [Self.middle, Self.corner])
    #expect(settled == [nil, .corner])
    // Back on the Screen, the stage's picture starts from the corner, under the small screen it grows out of, and
    // never in the middle, where it would stand apart from it.
    let coming = try ViewerHost.firstFrame(
      from: try Self.state(.activity),
      to: try Self.state(.screen),
      at: [Self.middle, Self.corner]
    )
    #expect(coming == [nil, .corner])
  }

  @Test("a switch to Activity turned straight back keeps one picture on the stage, never a gap")
  func aSwitchTurnedBackKeepsOnePicture() throws {
    let clock = ContinuousClock()
    var colours: [PictureColour?] = []
    // The two switches' frame must be drawn before the first switch's move has gone far, as a person turning straight
    // back sees it; a busy Mac that draws it later tries again in a fresh window.
    for _ in 0..<ViewerHost.firstFrameTries {
      let host = ViewerHost(try Self.state(.screen))
      let started = clock.now
      host.show(try Self.state(.activity))
      host.show(try Self.state(.screen))
      colours = try host.colours(at: [Self.middle, Self.corner])
      if clock.now - started <= ViewerHost.firstFrameWithin { break }
    }
    // The small screen had not left the stage's middle, so the picture coming back starts there, whole: the middle
    // shows one picture or the other, never the empty stage, and the corner holds no second small screen.
    #expect(colours[0] != nil, "the stage's middle was empty as the switch turned back")
    #expect(colours[1] == nil, "a small screen was left in the corner")
  }

  /// A point in the middle of the stage, clear of the corner.
  static let middle = CGPoint(x: PreviewStates.viewerSize.width / 3, y: PreviewStates.viewerSize.height / 2)
  /// The middle of the small screen in its corner: 26 from the stage's right and bottom edges, 248 by 155 in a window
  /// this wide.
  static let corner = CGPoint(x: PreviewStates.viewerSize.width - 150, y: PreviewStates.viewerSize.height - 103)

  /// The window on the Screen or on Activity with the small screen in its corner. The stage and the corner read one
  /// picture in the window; here each has its own, so the slot can paint them apart.
  ///
  /// - Parameter tab: The Screen, or Activity.
  /// - Returns: What the window shows.
  /// - Throws: When a picture cannot be made.
  static func state(_ tab: ViewerTab) throws -> ViewerState {
    var state = tab == .screen ? PreviewStates.viewerWorking : PreviewStates.viewerActivity
    state.screen.picture = try LayerPictureTests.picture(PictureColour.stage.rawValue)
    state.mini =
      tab.keepsScreenInCorner
      ? MiniScreenState(picture: try LayerPictureTests.picture(PictureColour.corner.rawValue)) : nil
    return state
  }
}
