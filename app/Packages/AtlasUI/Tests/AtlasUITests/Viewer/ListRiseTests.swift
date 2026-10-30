import SwiftUI
import Testing

@testable import AtlasUI

@Suite("ListRise")
struct ListRiseTests {
  @Test("a list arriving from the Screen starts 12 points low and clear")
  func aListFromTheScreenStartsLowAndClear() {
    for list in ViewerPlace.keptPlaces {
      #expect(ListRise.rises(from: .screen, to: list, reduceMotion: false), "\(list)")
      #expect(ListRise.rises(from: list, to: .screen, reduceMotion: false), "back from \(list)")
    }
    #expect(ListRise.offset(isLowered: true) == 12)
    #expect(ListRise.offset(isLowered: false) == 0)
    // It rises on the picture's spring, and fades in on the panes' own, starting clear.
    #expect(ListRise.motion == Motion.zoom)
    #expect(ScreenZoom.opacity(isArriving: true, isAway: true, zooms: false) == 0)
    #expect(PaneShift.arrival == Motion.swapIn)
    #expect(PaneShift.departure == Motion.swapOut)
  }

  @Test("a sideways step between lists only crossfades")
  func aSidewaysStepOnlyCrossfades() {
    for old in ViewerPlace.keptPlaces {
      for new in ViewerPlace.keptPlaces where new != old {
        #expect(!ListRise.rises(from: old, to: new, reduceMotion: false), "\(old) to \(new)")
      }
    }
    #expect(!ListRise.rises(from: .screen, to: .screen, reduceMotion: false))
  }

  @Test("under Reduce Motion a list only fades, from the Screen and back")
  func underReduceMotionAListOnlyFades() {
    for list in ViewerPlace.keptPlaces {
      #expect(!ListRise.rises(from: .screen, to: list, reduceMotion: true), "\(list)")
      #expect(!ListRise.rises(from: list, to: .screen, reduceMotion: true), "back from \(list)")
    }
  }
}
