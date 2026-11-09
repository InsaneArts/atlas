import SwiftUI
import Testing

@testable import AtlasUI

@Suite("PaneShift")
struct PaneShiftTests {
  @Test("a pane arrives and departs with opacity fades in place")
  func panesFadeInPlace() {
    // A pane away from its place is only transparent: an arriving pane starts clear, a leaving one ends clear.
    for isArriving in [true, false] {
      #expect(ScreenZoom.opacity(isArriving: isArriving, isAway: true, zooms: false) == 0)
      #expect(ScreenZoom.opacity(isArriving: isArriving, isAway: false, zooms: false) == 1)
    }
    // Activity is kept, and fades the same way.
    #expect(ViewerPlace.keptPlaces == [.activity])
  }
}
