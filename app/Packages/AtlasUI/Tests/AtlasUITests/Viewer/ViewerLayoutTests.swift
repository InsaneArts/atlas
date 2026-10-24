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
  @Test("on the Screen the stage makes Details' room on Details' spring, and at once elsewhere or under Reduce Motion")
  func theStageAndDetailsMoveTogether() {
    // The stage's room follows whether Details shows: Details' column and the window's inset beside it.
    #expect(ViewerLayout.stageTrailing(showsDetails: true) == 292)
    #expect(ViewerLayout.stageTrailing(showsDetails: false) == 0)
    #expect(ViewerLayout.detailsWidth == 280)
    // Details slides by a whole column: as far as the stage's edge moves, so on one spring the two edges keep
    // together at every point of the slide, and turn back together when it is toggled again mid-way (R17).
    let stageTravel = ViewerLayout.stageTrailing(showsDetails: true) - ViewerLayout.stageTrailing(showsDetails: false)
    let columnTravel =
      ViewerLayout.detailsOffset(isShown: false, reduceMotion: false)
      - ViewerLayout.detailsOffset(isShown: true, reduceMotion: false)
    #expect(stageTravel == columnTravel)
    #expect(ViewerLayout.stageSlide(tab: .screen, reduceMotion: false) == Motion.slide)
    #expect(ViewerLayout.detailsSlide(reduceMotion: false) == Motion.slide)
    #expect(Motion.slide == .spring(Motion.Springs.slide))
    // A list or a grid takes its new width at once rather than laying out again at every frame of the slide.
    for tab in ViewerTab.keptTabs {
      #expect(ViewerLayout.stageSlide(tab: tab, reduceMotion: false) == nil, "\(tab)")
    }
    // Under Reduce Motion Details fades in place and the stage changes at once.
    #expect(ViewerLayout.stageSlide(tab: .screen, reduceMotion: true) == nil)
    #expect(ViewerLayout.detailsSlide(reduceMotion: true) == Motion.reduced)
    #expect(ViewerLayout.detailsOffset(isShown: false, reduceMotion: true) == 0)
    #expect(ViewerLayout.detailsOpacity(isShown: false, reduceMotion: true) == 0)
    #expect(ViewerLayout.detailsOpacity(isShown: true, reduceMotion: true) == 1)
    #expect(ViewerLayout.detailsOpacity(isShown: false, reduceMotion: false) == 1)
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

  @Test("the window's blur behind a large screenshot never animates under Reduce Motion, and its shade only fades")
  func theLargeScreenshotsBlurIsStillUnderReduceMotion() {
    #expect(ViewerLayout.shotBlurring(reduceMotion: false) == ViewerLayout.shotFade)
    #expect(ViewerLayout.shotBlurring(reduceMotion: true) == nil)
    #expect(ViewerLayout.shotFading(reduceMotion: false) == ViewerLayout.shotFade)
    #expect(ViewerLayout.shotFading(reduceMotion: true) == Motion.reduced)
    #expect(ViewerLayout.shotBlur(isShown: true) == 6)
    #expect(ViewerLayout.shotBlur(isShown: false) == 0)
  }
}
