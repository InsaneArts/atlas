import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("DemonstrationBudget")
struct DemonstrationBudgetTests {
  /// A moment `seconds` after the start.
  private static func at(_ seconds: TimeInterval) -> Date {
    Date(timeIntervalSince1970: 1_000 + seconds)
  }

  @Test("seen without a pause, the ghost ends after its loops, once the last one has landed")
  func endsAfterItsLoops() {
    var budget = DemonstrationBudget()
    budget.appear(at: Self.at(0))
    let early = budget.allows(at: Self.at(5), loopStart: Self.at(0))
    #expect(early)
    let lastFlight = budget.allows(at: Self.at(DemonstrationBudget.limit - 0.5), loopStart: Self.at(0))
    #expect(lastFlight, "the last loop still flies")
    let afterwards = budget.allows(at: Self.at(DemonstrationBudget.limit + 0.1), loopStart: Self.at(0))
    #expect(!afterwards)
    #expect(budget.isSpent)
  }

  @Test("a pause while the person points at the icon does not use up the ghost's time")
  func pausesDoNotCount() {
    var budget = DemonstrationBudget()
    budget.appear(at: Self.at(0))
    budget.disappear(at: Self.at(4))
    budget.appear(at: Self.at(60))
    let back = budget.allows(at: Self.at(62), loopStart: Self.at(60))
    #expect(back, "4 s seen, not 62")
  }

  @Test("over its time mid-flight, the ghost finishes the loop and ends in its rest")
  func neverEndsMidFlight() {
    var budget = DemonstrationBudget()
    budget.appear(at: Self.at(0))
    budget.disappear(at: Self.at(7))
    budget.appear(at: Self.at(20))
    let flying = budget.allows(at: Self.at(21.5), loopStart: Self.at(20))
    #expect(flying, "past its time, but flying")
    let landed = budget.allows(at: Self.at(20 + DragGhostTrack.land + 0.1), loopStart: Self.at(20))
    #expect(!landed, "landed: it ends")
  }
}
