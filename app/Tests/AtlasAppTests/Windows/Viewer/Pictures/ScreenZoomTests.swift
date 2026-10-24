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
    let leaving = ViewerHost(try Self.state(.screen))
    // On the Screen, the stage's picture fills the middle of the window.
    #expect(try leaving.colour(at: Self.middle) == .stage)
    // Activity's first frame draws the small screen where the stage's picture was, whole over it: one picture, about
    // to shrink into the corner. Two pictures would leave the stage's own in the middle as it fades.
    leaving.show(try Self.state(.activity))
    #expect(try leaving.colour(at: Self.middle) == .corner)

    let coming = ViewerHost(try Self.state(.activity))
    // Over Activity, the small screen sits in its corner, drawn through the window's live slot (which paints its
    // picture blue here), and Activity has the middle.
    #expect(try coming.colour(at: Self.middle) == nil)
    #expect(try coming.colour(at: Self.corner) == .corner)
    // Back on the Screen, the stage's picture starts from the corner, under the small screen it grows out of, and
    // never in the middle, where it would stand apart from it.
    coming.show(try Self.state(.screen))
    #expect(try coming.colour(at: Self.middle) == nil)
    #expect(try coming.colour(at: Self.corner) == .corner)
  }

  @Test("a switch to Activity turned straight back keeps one picture on the stage, never a gap")
  func aSwitchTurnedBackKeepsOnePicture() throws {
    let host = ViewerHost(try Self.state(.screen))
    host.show(try Self.state(.activity))
    host.show(try Self.state(.screen))
    // The small screen had not left the stage's middle, so the picture coming back starts there, whole: the middle
    // shows one picture or the other, never the empty stage, and the corner holds no second small screen.
    #expect(try host.colour(at: Self.middle) != nil, "the stage's middle was empty as the switch turned back")
    #expect(try host.colour(at: Self.corner) == nil, "a small screen was left in the corner")
  }

  /// A point in the middle of the stage, clear of the corner.
  static let middle = CGPoint(x: PreviewStates.viewerSize.width / 3, y: PreviewStates.viewerSize.height / 2)
  /// The middle of the small screen in its corner: 26 from the window's right and bottom edges, 248 by 155.
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
