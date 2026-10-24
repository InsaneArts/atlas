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

  @Test("a kept pane leaves away from the new tab, waits on the side its tab lies, and turns back the way it went")
  func keptPanesWaitOnTheirOwnSide() {
    // Activity to Apps: Activity leaves 12 to the left, Screenshots waits 16 to the right, Apps sits in place.
    #expect(ViewerLayout.keptPaneOffset(.activity, shown: .apps, left: .activity, reduceMotion: false) == -12)
    #expect(ViewerLayout.keptPaneOffset(.screenshots, shown: .apps, left: .activity, reduceMotion: false) == 16)
    #expect(ViewerLayout.keptPaneOffset(.apps, shown: .apps, left: .activity, reduceMotion: false) == 0)
    // Back to Activity mid-way: it comes in from the left, the side it left by, so it turns back rather than jumping
    // across; Apps leaves to the right.
    #expect(ViewerLayout.keptPaneOffset(.activity, shown: .apps, left: nil, reduceMotion: false) == -16)
    #expect(ViewerLayout.keptPaneOffset(.apps, shown: .activity, left: .apps, reduceMotion: false) == 12)
    // From the Screen every kept pane waits to the right, as every one of them lies there.
    for tab in ViewerTab.keptTabs {
      #expect(ViewerLayout.keptPaneOffset(tab, shown: .screen, left: nil, reduceMotion: false) == 16, "\(tab)")
      #expect(ViewerLayout.keptPaneOffset(tab, shown: .screen, left: tab, reduceMotion: false) == 12, "\(tab)")
    }
    // Under Reduce Motion they only crossfade.
    #expect(ViewerLayout.keptPaneOffset(.activity, shown: .apps, left: .activity, reduceMotion: true) == 0)
    #expect(ViewerLayout.keptPaneOffset(.screenshots, shown: .apps, left: nil, reduceMotion: true) == 0)
    // Every tab but the Screen is kept.
    #expect(ViewerTab.keptTabs == [.activity, .apps, .screenshots])
  }
}
