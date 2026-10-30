import CoreGraphics
import SwiftUI
import Testing

@testable import AtlasUI

@Suite("PictureRing")
@MainActor
struct PictureRingTests {
  @Test("the selection ring stands outside the tile, concentric with its corner")
  func theRingStandsOutsideTheTileConcentricWithItsCorner() throws {
    // Its inner edge runs the gap outside the picture's corner and its outer edge its width further.
    #expect(PictureRing.reach == PictureRing.gap + PictureRing.width)
    #expect(PictureRing.outerRadius(around: 10) == 10 + PictureRing.reach)
    let picture = try #require(ShotThumbnailTests.picture(width: 100, height: 60))
    let selected = try Pixels(
      PictureTile(picture: picture, isSelected: true, showsEdge: false)
        .frame(width: 100, height: 60)
        .padding(Self.room)
    )
    let plain = try Pixels(
      PictureTile(picture: picture, showsEdge: false)
        .frame(width: 100, height: 60)
        .padding(Self.room)
    )
    let middle = Int(Self.room) + 30
    // The picture is drawn just the same selected or not: nothing over it, nothing moved or shrunk.
    for x in [Int(Self.room) + 1, Int(Self.room) + 50, Int(Self.room) + 98] {
      let same = selected.colour(x: x, y: middle) == plain.colour(x: x, y: middle)
      #expect(same, "the ring drew over the picture at \(x)")
    }
    // Outside its edge, the ground shows through the gap, then the accent ring.
    let edge = Int(Self.room)
    #expect(selected.colour(x: edge - 2, y: middle).alpha < 0.1, "no gap between the picture and the ring")
    let ring = selected.colour(x: edge - Int(PictureRing.gap) - 1, y: middle)
    #expect(ring.blue > 0.8 && ring.red < 0.3 && ring.alpha > 0.9, "no blue ring outside the picture")
    #expect(selected.colour(x: edge - Int(PictureRing.reach) - 2, y: middle).alpha < 0.1, "a second ring or a halo")
  }

  /// 12: the room around the picture the ring draws in.
  static let room: CGFloat = 12
}
