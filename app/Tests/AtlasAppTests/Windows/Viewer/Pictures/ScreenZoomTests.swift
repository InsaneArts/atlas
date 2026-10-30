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
      to: try Self.state(.activity)
    ) { host in
      try host.render(until: "the incoming small screen has not been laid out at the held first frame") {
        host.frames["picture \(PictureColour.corner.rawValue)"] != nil
      }
      return try host.colours(at: [Self.middle])
    }
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
      to: try Self.state(.screen)
    ) { host in
      try host.render(until: "the incoming stage picture has not been laid out at the held first frame") {
        host.frames["picture \(PictureColour.stage.rawValue)"] != nil
      }
      return try host.colours(at: [Self.middle, Self.corner])
    }
    #expect(coming == [nil, .corner])
  }

  @Test(
    "a switch to a list turned straight back keeps one picture on the stage, never a gap",
    arguments: [ViewerPlace.activity, .screenshots]
  )
  func aSwitchTurnedBackKeepsOnePicture(list: ViewerPlace) throws {
    let previous = Motion.Pose.moment
    Motion.Pose.hold(at: .zero)
    defer { Motion.Pose.hold(at: previous) }
    let host = ViewerHost(try Self.state(.screen))
    host.show(try Self.state(list))
    host.show(try Self.state(.screen))
    let colours = try host.colours(at: [Self.middle, Self.corner])
    // The small screen had not left the stage's middle, so the picture coming back starts there, whole: the middle
    // shows one picture or the other, never the empty stage, and the corner holds no second small screen.
    #expect(colours[0] != nil, "the stage's middle was empty as the switch turned back")
    #expect(colours[1] == nil, "a small screen was left in the corner")
  }

  /// A point in the middle of the stage, clear of the corner.
  static let middle = CGPoint(x: PreviewStates.viewerSize.width / 3, y: PreviewStates.viewerSize.height / 2)
  /// A point inside the small screen in its corner of the card's body, above the footer: 200 across, 12 in from the
  /// card's edge, which is 8 in from the window's, and its foot 12 over the footer's 46.
  static let corner = CGPoint(x: PreviewStates.viewerSize.width - 120, y: PreviewStates.viewerSize.height - 120)

  /// The window on the Screen, or on Activity or Screenshots with the small screen in its corner. The stage and the
  /// corner read one picture in the window; here each has its own, so the slot can paint them apart.
  ///
  /// - Parameter place: The Screen, Activity or Screenshots.
  /// - Returns: What the window shows.
  /// - Throws: When a picture cannot be made.
  static func state(_ place: ViewerPlace) throws -> ViewerState {
    var state =
      switch place {
      case .screen: PreviewStates.viewerWorking
      case .screenshots: PreviewStates.viewerShots
      case .activity, .apps: PreviewStates.viewerActivity
      }
    state.screen.picture = try LayerPictureTests.picture(PictureColour.stage.rawValue)
    state.mini =
      place.keepsScreenInCorner
      ? MiniScreenState(picture: try LayerPictureTests.picture(PictureColour.corner.rawValue)) : nil
    return state
  }
}
