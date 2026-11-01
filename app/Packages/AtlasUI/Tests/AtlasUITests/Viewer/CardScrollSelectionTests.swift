import SwiftUI
import Testing

@testable import AtlasUI

@Suite("CardScroll selection store")
@MainActor
struct CardScrollSelectionTests {
  @Test("an oversized final tile finishes revealing before the person scrolls", arguments: [250.0, 300.0])
  func anOversizedFinalTileFinishesRevealingBeforeThePersonScrolls(height: CGFloat) throws {
    let selection = CardScrollSelection()
    let viewport: CGFloat = 400
    let clearance = MiniClearance(edges: .top, length: 149)
    let room = viewport - height
    let last = CGRect(x: 0, y: room - ViewerLayout.swapInset, width: 100, height: height)
    selection.report("tile", at: last)
    var corrections: [UnitPoint] = []
    selection.place("tile") { frame in
      guard let anchor = CardScrollRoom.revealAnchor(row: frame, viewport: viewport, clearance: clearance) else {
        selection.cancelPlacement()
        return
      }
      corrections.append(anchor)
    }
    let anchor = try #require(corrections.first)
    let destination = max(last.minY, anchor.y * room)
    let arrived = CGRect(x: last.minX, y: destination, width: last.width, height: height)
    selection.report("tile", at: arrived)
    #expect(corrections.count == 1)
    #expect(CardScrollRoom.revealAnchor(row: arrived, viewport: viewport, clearance: clearance) == nil)
    selection.report("tile", at: arrived.offsetBy(dx: 0, dy: 40))
    #expect(corrections.count == 1)
  }

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
