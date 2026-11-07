import SwiftUI
import Testing

@testable import AtlasUI

@Suite("Outline pictures")
@MainActor
struct OutlinePictureTests {
  @Test("a window capture stands inside its frame on the recessed ground, and the whole screen fills it")
  func aWindowStandsInsideAndTheScreenFills() throws {
    let picture = try #require(TestPictures.picture(width: 160, height: 100))
    let size = ViewerLayout.pictureGrid
    let inset = ViewerLayout.pictureGridInset
    let window = try Pixels(
      OutlinePicture(tile: ViewerPictureTile(id: "w", thumbnail: picture, label: ""), size: size, inset: inset)
    )
    let screen = try Pixels(
      OutlinePicture(
        tile: ViewerPictureTile(id: "s", thumbnail: picture, fillsFrame: true, label: ""),
        size: size,
        inset: inset
      )
    )
    let middle = Int(size.height / 2)
    // Just inside the rim the window leaves the ground showing, where the whole screen draws its own edge.
    let edge = 1
    #expect(window.colour(x: edge, y: middle).green < 0.5, "the window capture reached the frame's edge")
    #expect(screen.colour(x: edge, y: middle).green > 0.8, "the whole screen left a margin")
    // Both draw the picture's middle.
    #expect(window.colour(x: Int(size.width / 2), y: middle).red > 0.8)
    #expect(screen.colour(x: Int(size.width / 2), y: middle).red > 0.8)
  }

  @Test("every frame is 16:10 and a strip shows fewer pictures beside its link as the card narrows")
  func framesAreSixteenByTenAndStripsNarrow() {
    for frame in [ViewerLayout.pictureFolded, ViewerLayout.pictureGrid, ViewerLayout.pictureInPlace] {
      #expect(frame.width / frame.height == 1.6)
    }
    #expect(ViewerLayout.stripCount(width: 900) == 3)
    #expect(ViewerLayout.stripCount(width: 560) == 2)
    #expect(ViewerLayout.stripCount(width: 420) == 1)
    #expect(ViewerLayout.outlineIndent == 28)
    #expect([0, 1, 2].map { ViewerLayout.outlineText(level: $0) } == [58, 86, 114])
    #expect(ViewerLayout.pictureGridWidth == 808)
    #expect(ViewerLayout.sheetWidth(window: 1200) == 824 && ViewerLayout.sheetWidth(window: 500) == 468)
  }
}
