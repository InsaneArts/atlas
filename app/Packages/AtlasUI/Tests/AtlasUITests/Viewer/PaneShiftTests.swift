import SwiftUI
import Testing

@testable import AtlasUI

@Suite("PaneShift")
struct PaneShiftTests {
  @Test("a new tab's pane fades in over the old one at once while the old one fades out quicker, nothing sliding")
  func panesCrossfadeInPlace() {
    #expect(PaneShift.arrival == Animation.easeOut(duration: 0.15))
    #expect(PaneShift.departure == Animation.easeIn(duration: 0.1))
    // A pane away from its place is only transparent: an arriving pane starts clear, a leaving one ends clear.
    for isArriving in [true, false] {
      #expect(ScreenZoom.opacity(isArriving: isArriving, isAway: true, zooms: false) == 0)
      #expect(ScreenZoom.opacity(isArriving: isArriving, isAway: false, zooms: false) == 1)
    }
    // Every tab but the Screen is kept, and fades the same way.
    #expect(ViewerTab.keptTabs == [.activity, .apps, .screenshots])
  }
}
