import Testing

@testable import AtlasUI

@Suite("PaneShift")
struct PaneShiftTests {
  @Test("moving right through the tabs slides panes from the right, and back slides them from the left")
  func panesSlideWithTheTabs() {
    let tabs = ViewerTab.allCases
    for (left, right) in zip(tabs, tabs.dropFirst()) {
      let onward = ViewerTab.direction(from: left, to: right)
      #expect(ViewerLayout.paneOffset(isArriving: true, direction: onward, reduceMotion: false) == 16, "\(right)")
      #expect(ViewerLayout.paneOffset(isArriving: false, direction: onward, reduceMotion: false) == -12, "\(left)")
      let back = ViewerTab.direction(from: right, to: left)
      #expect(ViewerLayout.paneOffset(isArriving: true, direction: back, reduceMotion: false) == -16, "\(left)")
      #expect(ViewerLayout.paneOffset(isArriving: false, direction: back, reduceMotion: false) == 12, "\(right)")
    }
    // A switch across several tabs goes the same way as one step; a tab to itself goes nowhere.
    #expect(ViewerTab.direction(from: .screen, to: .screenshots) == 1)
    #expect(ViewerTab.direction(from: .screenshots, to: .activity) == -1)
    #expect(ViewerTab.direction(from: .apps, to: .apps) == 0)
    // Under Reduce Motion the panes only crossfade.
    for isArriving in [true, false] {
      #expect(ViewerLayout.paneOffset(isArriving: isArriving, direction: 1, reduceMotion: true) == 0)
    }
    // The old pane shows alone for 50 milliseconds before the new one starts in, so the stage is never blank.
    #expect(ViewerLayout.paneOverlap == 0.05)
  }
}
