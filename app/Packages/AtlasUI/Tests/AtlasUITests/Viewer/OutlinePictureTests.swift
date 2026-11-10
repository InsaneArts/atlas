import SwiftUI
import Testing

@testable import AtlasUI

@Suite("Outline pictures")
@MainActor
struct OutlinePictureTests {
  @Test("a picture fills its frame from its top, cut at the frame's edge, whatever its shape")
  func aPictureFillsItsFrameFromItsTop() throws {
    let size = ViewerLayout.pictureGrid
    let wide = try #require(TestPictures.picture(width: 200, height: 100))
    let tall = try #require(TestPictures.picture(width: 100, height: 100))
    let filled = try Pixels(OutlinePicture(tile: ViewerPictureTile(id: "w", thumbnail: wide, label: ""), size: size))
    let square = try Pixels(OutlinePicture(tile: ViewerPictureTile(id: "t", thumbnail: tall, label: ""), size: size))
    let middle = Int(size.height / 2)
    // No ground shows anywhere: a wide picture's middle reaches both sides, cut there.
    #expect(filled.colour(x: 1, y: middle).red > 0.8, "a wide picture stood inside its frame")
    #expect(filled.colour(x: Int(size.width) - 2, y: middle).red > 0.8)
    // A taller picture is cut at the bottom, its top kept: its green columns reach the frame's edges.
    #expect(square.colour(x: 1, y: 2).green > 0.8, "the picture was not anchored at its top")
  }

  @Test("every frame is 16:10 and a strip shows fewer pictures beside its link as the card narrows")
  func framesAreSixteenByTenAndStripsNarrow() {
    for frame in [ViewerLayout.pictureFolded, ViewerLayout.pictureGrid, ViewerLayout.pictureInPlace] {
      #expect(frame.width / frame.height == 1.6)
    }
    #expect(ViewerLayout.stripCount(width: 900) == 3)
    #expect(ViewerLayout.stripCount(width: 560) == 2)
    #expect(ViewerLayout.stripCount(width: 420) == 1)
    #expect(ViewerLayout.outlineIndent == 26)
    #expect([0, 1, 2].map { ViewerLayout.outlineText(level: $0) } == [66, 92, 118])
    // A child's chevron stands centred under its parent's lead: 12 + 26 + 6 is 12 + 12 + 8 + 12.
    let childChevron = ViewerLayout.outlineInset + ViewerLayout.outlineIndent + ViewerLayout.outlineDisclosure / 2
    let parentLead =
      ViewerLayout.outlineInset + ViewerLayout.outlineDisclosure + ViewerLayout.outlineDisclosureGap
      + ViewerLayout.outlineLead / 2
    #expect(childChevron == parentLead)
    #expect(ViewerLayout.pictureGridWidth == 808)
    #expect(ViewerLayout.sheetWidth(window: 1200) == 824 && ViewerLayout.sheetWidth(window: 500) == 468)
  }
}
