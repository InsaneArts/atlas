import CoreGraphics
import Testing

@testable import AtlasUI

@Suite("CardScroll selection store")
@MainActor
struct CardScrollSelectionTests {
  @Test("a reselected row waits for a frame reported after its last deselection")
  func aReselectedRowWaitsForFreshGeometry() {
    let selection = CardScrollSelection()
    let old = CGRect(x: 0, y: 20, width: 100, height: 40)
    let current = old.offsetBy(dx: 0, dy: 500)
    selection.report("tile", at: old)
    selection.forget("tile")
    #expect(selection.id == nil)
    #expect(selection.frame == nil)
    var placed: [CGRect] = []
    #expect(!selection.place("tile") { placed.append($0) })
    #expect(placed.isEmpty)
    selection.report("tile", at: current)
    #expect(placed == [current])
  }

  @Test("forgetting a row cancels a placement waiting for its first geometry report")
  func forgettingARowCancelsItsWaitingPlacement() {
    let selection = CardScrollSelection()
    var placed: [CGRect] = []
    #expect(!selection.place("tile") { placed.append($0) })
    selection.forget("tile")
    selection.report("tile", at: .zero)
    #expect(placed.isEmpty)
  }

  @Test("a late deselection leaves another row's waiting placement intact")
  func forgettingAnOldRowPreservesTheNextPlacement() {
    let selection = CardScrollSelection()
    selection.report("old", at: .zero)
    var placed: [CGRect] = []
    #expect(!selection.place("next") { placed.append($0) })
    selection.forget("old")
    selection.report("next", at: .zero)
    #expect(placed == [.zero])
  }
}
