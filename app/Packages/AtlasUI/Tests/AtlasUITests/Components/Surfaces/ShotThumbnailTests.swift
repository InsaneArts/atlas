import CoreGraphics
import SwiftUI
import Testing

@testable import AtlasUI

@Suite("ShotThumbnail")
@MainActor
struct ShotThumbnailTests {
  @Test("a screenshot tile shows its whole picture at its own shape, never cut or zoomed")
  func aTileShowsItsWholePictureAtItsOwnShape() throws {
    // A 2:1 picture, green at its left and right edges and red between, in a room of the grid's 16:10.
    let picture = try #require(Self.picture(width: 200, height: 100))
    let shape = ShotShape.of(picture, listed: nil)
    #expect(shape == 2)
    let drawn = try Pixels(
      ShotThumbnail(picture: picture, shape: shape, radius: .zero, showsEdge: false)
        .frame(width: 160, height: 100)
    )
    // It stands 160 by 80 at the room's foot: both green edges show, so nothing was cut, and the top 20 is empty.
    #expect(Pixels.isGreen(drawn.colour(x: 1, y: 60)), "its left edge was cut")
    #expect(Pixels.isGreen(drawn.colour(x: 158, y: 60)), "its right edge was cut")
    #expect(Pixels.isRed(drawn.colour(x: 80, y: 60)))
    #expect(drawn.colour(x: 80, y: 10).alpha == 0, "it was stretched or zoomed to the room")
    #expect(drawn.colour(x: 80, y: 22).alpha > 0.9)
  }

  @Test("a thumbnail takes its picture's own shape, else the list's, else 16 by 10")
  func aThumbnailTakesItsPicturesShape() throws {
    let picture = try #require(Self.picture(width: 300, height: 400))
    #expect(ShotShape.of(picture, listed: 1.6) == 0.75, "the picture read says its own shape")
    #expect(Self.same(ShotShape.of(nil, listed: 1512.0 / 982.0), 1512.0 / 982.0))
    #expect(ShotShape.of(nil, listed: nil) == 1.6)
    #expect(ShotShape.of(nil, listed: 0) == 1.6, "no shape is no shape")
  }

  @Test("the grid's places take the shape most screenshots share, kept between 4:3 and 16:9")
  func theGridsPlacesTakeTheCommonShape() {
    let tile = { (aspect: CGFloat?) in ViewerShotTile(id: UUID().uuidString, time: "", aspect: aspect, label: "") }
    let state = { (aspects: [CGFloat?]) in
      ViewerShotsState(meta: "", days: [ViewerShotDay(id: "hour", title: "Now", shots: aspects.map(tile))])
    }
    #expect(Self.same(state([1512.0 / 982.0, 1512.0 / 982.0, 4.0 / 3.0]).tileShape, 1.54))
    #expect(Self.same(state([4.0 / 3.0, 1.6]).tileShape, 4.0 / 3.0), "the newest first among equals")
    #expect(Self.same(state([0.6, 0.6]).tileShape, 4.0 / 3.0), "a tall window never makes every row a tower")
    #expect(Self.same(state([3, 3]).tileShape, 16.0 / 9.0))
    #expect(Self.same(state([nil]).tileShape, 1.6))
  }

  /// Whether two shapes are the same, but for the last bits a division leaves.
  ///
  /// - Parameters:
  ///   - lhs: One shape.
  ///   - rhs: The other.
  /// - Returns: Whether they are.
  static func same(_ lhs: CGFloat, _ rhs: CGFloat) -> Bool {
    abs(lhs - rhs) < 0.000_001
  }

  /// A picture `width` by `height`, green in its outer four columns on each side and red between.
  ///
  /// - Parameters:
  ///   - width: Its width in pixels.
  ///   - height: Its height in pixels.
  /// - Returns: The picture.
  static func picture(width: Int, height: Int) -> DesktopPicture? {
    guard
      let context = CGContext(
        data: nil,
        width: width,
        height: height,
        bitsPerComponent: 8,
        bytesPerRow: 0,
        space: CGColorSpace(name: CGColorSpace.sRGB) ?? CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
      )
    else { return nil }
    context.setFillColor(red: 1, green: 0, blue: 0, alpha: 1)
    context.fill(CGRect(x: 0, y: 0, width: width, height: height))
    context.setFillColor(red: 0, green: 1, blue: 0, alpha: 1)
    context.fill(CGRect(x: 0, y: 0, width: 4, height: height))
    context.fill(CGRect(x: width - 4, y: 0, width: 4, height: height))
    return context.makeImage()
      .map { DesktopPicture(id: "\(width)x\(height)", image: $0) }
  }
}
