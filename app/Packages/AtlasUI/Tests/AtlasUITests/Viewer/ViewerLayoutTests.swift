import CoreGraphics
import Testing

@testable import AtlasUI

@Suite("ViewerLayout")
struct ViewerLayoutTests {
  @Test("the top row draws in one step at a time as the window narrows, never skipping or undoing one")
  func theTopRowDrawsInStepByStep() {
    let widths = stride(from: CGFloat(1440), through: 700, by: -10)
    let steps = widths.map(ViewerLayout.topRowFit(width:))
    #expect(steps == steps.sorted(), "a narrower window never draws in less")
    #expect(Set(steps) == [.regular, .keysGlyph, .tightTabs, .statusWord, .shortWords], "every step is reached")
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
      // 800 by 500 leaves 640 across beside Previous and Next, and 380 down above the bar.
      (CGSize(width: 16, height: 10), CGSize(width: 800, height: 500), CGSize(width: 608, height: 380)),
    ] as [(CGSize, CGSize, CGSize)]
  )
  func largeShotSize(aspect: CGSize, window: CGSize, size: CGSize) {
    #expect(ViewerLayout.largeShotSize(aspect: aspect, in: window) == size)
  }
}
