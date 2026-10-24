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
      // 800 by 508 leaves 640 across beside Previous and Next, and 360 down, centred clear of the bar at its foot.
      (CGSize(width: 16, height: 10), CGSize(width: 800, height: 508), CGSize(width: 576, height: 360)),
    ] as [(CGSize, CGSize, CGSize)]
  )
  func largeShotSize(aspect: CGSize, window: CGSize, size: CGSize) {
    #expect(ViewerLayout.largeShotSize(aspect: aspect, in: window) == size)
  }
}

extension ViewerLayoutTests {
  @Test("the stage keeps its new width while Details slides")
  func theStageKeepsItsNewWidthWhileDetailsSlides() {
    // The stage's room follows whether Details shows, and nothing else: there is no slide in it.
    #expect(ViewerLayout.stageTrailing(showsDetails: true) == 292)
    #expect(ViewerLayout.stageTrailing(showsDetails: false) == 0)
    #expect(ViewerLayout.detailsWidth == 280)
    // Details slides by an offset of its own: hidden, a whole column out past the window's edge, so at every point of
    // the slide its leading edge stays at or beyond the stage's new trailing edge, and the stage never waits for it.
    let hidden = ViewerLayout.detailsOffset(isShown: false, reduceMotion: false)
    #expect(hidden == ViewerLayout.detailsColumn)
    #expect(ViewerLayout.detailsOffset(isShown: true, reduceMotion: false) == 0)
    let window: CGFloat = 1440
    let stageEdge = window - ViewerLayout.stageTrailing(showsDetails: true)
    for step in 0...10 {
      let offset = hidden * CGFloat(step) / 10
      #expect(window - ViewerLayout.detailsColumn + offset >= stageEdge)
    }
    // Under Reduce Motion it fades in place instead.
    #expect(ViewerLayout.detailsOffset(isShown: false, reduceMotion: true) == 0)
    #expect(ViewerLayout.detailsOpacity(isShown: false, reduceMotion: true) == 0)
    #expect(ViewerLayout.detailsOpacity(isShown: true, reduceMotion: true) == 1)
    #expect(ViewerLayout.detailsOpacity(isShown: false, reduceMotion: false) == 1)
    // The slide is a spring, which turns back from where it is when Details is toggled again mid-way (R17).
    #expect(Motion.slide == .spring(Motion.Springs.slide))
  }

  @Test("the spotlight's label moves inside the outline at the right edge")
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
    // A label wider than the picture is cut to it.
    let wide = OutlineLabelPlacement.frame(label: CGSize(width: 2000, height: 28), outline: edge, in: picture)
    #expect(wide == CGRect(x: 8, y: 208, width: 984, height: 28))
  }
}
