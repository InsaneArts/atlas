import SwiftUI
import Testing

@testable import AtlasUI

@Suite("Action slots")
struct ActionSlotTests {
  @Test("a control leaves before its neighbours close the gap, and comes only once they have made room for it")
  func aControlNeverPassesUnderItsNeighbours() {
    // The list's three beats: out, then the row moves, then in, so a neighbour sliding never crosses a control still
    // drawn or one already fading in.
    #expect(ActionSlot.departure == Motion.swapOut)
    #expect(ActionSlot.animation(reduceMotion: false) == Motion.listMove)
    #expect(ActionSlot.arrival == Motion.listIn)
  }

  @Test("under Reduce Motion the row only fades")
  func underReduceMotionTheRowOnlyFades() {
    #expect(ActionSlot.animation(reduceMotion: true) == Motion.reduced)
  }
}
