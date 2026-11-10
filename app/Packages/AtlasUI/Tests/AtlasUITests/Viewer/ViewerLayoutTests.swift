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

  @Test("a large screenshot's shade fades in over 0.2 seconds, with nothing blurred under it")
  func theLargeScreenshotsShadeOnlyFades() {
    #expect(ViewerLayout.replayFade == Animation.easeOut(duration: 0.2))
    #expect(ViewerLayout.replayScrim == 0.72)
  }
}
