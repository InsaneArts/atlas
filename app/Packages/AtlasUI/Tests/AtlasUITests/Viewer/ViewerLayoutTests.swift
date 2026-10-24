import CoreGraphics
import Testing

@testable import AtlasUI

@Suite("ViewerLayout")
struct ViewerLayoutTests {
  @Test("a narrow window drops the status line before the tab words, and the tab words before the action words")
  func aNarrowWindowCollapsesInOrder() throws {
    let layouts = ViewerLayout.barLayouts
    #expect(layouts.first == .full, "the widest layout shows everything")
    let noLine = try #require(layouts.firstIndex { !$0.reservesLine })
    let tabSymbols = try #require(layouts.firstIndex { $0.tabFace == .symbol })
    let actionSymbols = try #require(layouts.firstIndex { !$0.actionWords })
    #expect(noLine < tabSymbols)
    #expect(tabSymbols < actionSymbols)
    for (wider, narrower) in zip(layouts, layouts.dropFirst()) {
      #expect(wider.reservesLine || !narrower.reservesLine, "a line once given up stays given up")
      #expect(wider.tabFace == .word || narrower.tabFace == .symbol, "tab words once dropped stay dropped")
      #expect(wider.actionWords || !narrower.actionWords, "action words once dropped stay dropped")
    }
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
