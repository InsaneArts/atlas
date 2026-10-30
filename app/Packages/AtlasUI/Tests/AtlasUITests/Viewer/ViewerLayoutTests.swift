import CoreGraphics
import SwiftUI
import Testing

@testable import AtlasUI

@Suite("ViewerLayout")
struct ViewerLayoutTests {
  @Test(
    "the card sits 8 in from the window at the nested corner, a 38-point header over the stage and a 46-point footer"
  )
  func theCardNestsInTheWindow() {
    #expect(ViewerLayout.cardInset == 8)
    #expect(ViewerLayout.cardRadius == Radius.nested(in: Radius.window, inset: ViewerLayout.cardInset))
    #expect(ViewerLayout.headerHeight == 38)
    #expect(ViewerLayout.footerHeight == 46)
    #expect(ViewerLayout.screenRadius == 6)
    #expect(ViewerLayout.controlEdge == 3)
  }

  @Test("a window outline is letterboxed as the picture is, so it lies where a click on that window lands")
  func outlineFrameLetterboxes() {
    // A 16:9 picture in a square view shows 400 × 225, from 87.5 down, as a click maps it.
    let frame = ViewerLayout.outlineFrame(
      CGRect(x: 0.5, y: 0.2, width: 0.25, height: 0.4),
      in: CGSize(width: 400, height: 400),
      picture: CGSize(width: 3200, height: 1800)
    )
    #expect(frame == CGRect(x: 200, y: 132.5, width: 100, height: 90))
    #expect(ViewerLayout.outlineFrame(frame, in: .zero, picture: CGSize(width: 1, height: 1)) == .zero)
  }

  @Test(
    "a large screenshot keeps its shape, no larger than 1040 by 650 and within the window beside its buttons and bar",
    arguments: [
      (CGSize(width: 16, height: 10), CGSize(width: 1440, height: 900), CGSize(width: 1040, height: 650)),
      (CGSize(width: 16, height: 9), CGSize(width: 1440, height: 900), CGSize(width: 1040, height: 585)),
      // 800 by 508 leaves 640 across beside Previous and Next, and 360 down, centred clear of the bar at its foot.
      (CGSize(width: 16, height: 10), CGSize(width: 800, height: 508), CGSize(width: 576, height: 360)),
    ] as [(CGSize, CGSize, CGSize)]
  )
  func largeShotSize(aspect: CGSize, window: CGSize, size: CGSize) {
    #expect(ViewerLayout.largeShotSize(aspect: aspect, in: window) == size)
  }
}

extension ViewerLayoutTests {
  @Test("the small screen is 200 by 125 in a wide card and never under 140 across")
  func theSmallScreenIsFixedInAWideCard() {
    let wide = ViewerLayout.pipSize(bodyWidth: 1416, aspect: 16.0 / 10.0)
    #expect(wide == CGSize(width: 200, height: 125))
    #expect(ViewerLayout.pipSize(bodyWidth: 560, aspect: nil) == wide, "with no picture yet it is 16 by 10")
    // Narrower than 560 it shrinks with the card's body, down to 140 across.
    #expect(ViewerLayout.pipSize(bodyWidth: 476, aspect: 16.0 / 10.0) == CGSize(width: 170, height: 106))
    #expect(ViewerLayout.pipSize(bodyWidth: 300, aspect: 16.0 / 10.0).width == 140)
    #expect(ViewerLayout.pipSize(bodyWidth: .zero, aspect: 16.0 / 10.0).width == 140)
    // It takes the helper's own shape, so the picture fills it, within 4:3 and 16:9.
    #expect(ViewerLayout.pipSize(bodyWidth: 1416, aspect: 1512.0 / 982.0) == CGSize(width: 200, height: 130))
    #expect(ViewerLayout.pipSize(bodyWidth: 1416, aspect: 3).height == 113)
    #expect(ViewerLayout.pipSize(bodyWidth: 1416, aspect: 0.5).height == 150)
    // The lists keep clear of its height and its inset on both sides of it, with its corner rounder than the stage's.
    #expect(ViewerLayout.pipRoom(wide) == 149)
    #expect(ViewerLayout.pipRadius == 9 && ViewerLayout.screenRadius == 6)
  }

  @Test("the spotlight's label moves inside the outline at the right edge, and never past the picture's bottom")
  func theSpotlightsLabelMovesInsideTheOutlineAtTheRightEdge() {
    let picture = CGSize(width: 1000, height: 600)
    let label = CGSize(width: 300, height: 28)
    // In the middle it stands above the outline at its left edge, its bottom 10 over the outline's top.
    let middle = CGRect(x: 400, y: 200, width: 200, height: 200)
    #expect(
      OutlineLabelPlacement.frame(label: label, outline: middle, in: picture)
        == CGRect(x: 400, y: 162, width: 300, height: 28)
    )
    // At the right edge above would run past the picture, so it moves inside, 8 from the outline's top and right.
    let edge = CGRect(x: 850, y: 200, width: 150, height: 300)
    #expect(
      OutlineLabelPlacement.frame(label: label, outline: edge, in: picture)
        == CGRect(x: 1000 - 8 - 300, y: 208, width: 300, height: 28)
    )
    // At the top edge too, kept 8 inside the picture.
    let top = CGRect(x: 100, y: 10, width: 200, height: 200)
    #expect(
      OutlineLabelPlacement.frame(label: label, outline: top, in: picture)
        == CGRect(x: 8, y: 18, width: 300, height: 28)
    )
    // At the bottom right, inside would run below the picture, so it stops 8 above the picture's bottom.
    let low = CGRect(x: 850, y: 580, width: 150, height: 20)
    #expect(
      OutlineLabelPlacement.frame(label: label, outline: low, in: picture)
        == CGRect(x: 1000 - 8 - 300, y: 600 - 8 - 28, width: 300, height: 28)
    )
    // A window partly off the screen at the bottom right keeps its label whole on the picture.
    let off = CGRect(x: 900, y: 590, width: 400, height: 300)
    let placed = OutlineLabelPlacement.frame(label: label, outline: off, in: picture)
    #expect(placed.maxY <= 600 - 8 && placed.maxX <= 1000 - 8 && placed.minX >= 8 && placed.minY >= 8, "\(placed)")
    // A label wider than the picture is cut to it.
    let wide = OutlineLabelPlacement.frame(label: CGSize(width: 2000, height: 28), outline: edge, in: picture)
    #expect(wide == CGRect(x: 8, y: 208, width: 984, height: 28))
  }

  @Test("a large screenshot's shade fades in over 0.2 seconds, with nothing blurred under it")
  func theLargeScreenshotsShadeOnlyFades() {
    #expect(ViewerLayout.shotFade == Animation.easeOut(duration: 0.2))
    #expect(ViewerLayout.shotScrim == 0.72)
  }
}
