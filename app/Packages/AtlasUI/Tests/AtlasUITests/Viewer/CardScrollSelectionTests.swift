import SwiftUI
import Testing

@testable import AtlasUI

@Suite("CardScroll selection store")
@MainActor
struct CardScrollSelectionTests {
  @Test("a queued reveal uses the latest tile height and waits for geometry to confirm it", arguments: [false, true])
  func aQueuedRevealWaitsForGeometry(hasGeometry: Bool) async throws {
    let selection = CardScrollSelection()
    let viewport: CGFloat = 400
    let clearance = MiniClearance(edges: .bottom, length: 149)
    let first = CGRect(x: 0, y: 300, width: 100, height: 60)
    let taller = CGRect(x: 0, y: 300, width: 100, height: 120)
    var tasks: [Task<Void, Never>] = []
    var placements: [UnitPoint] = []
    var reports: [CGRect] = []
    let placement = CardScrollPlacement { place in
      let task = Task { @MainActor in
        guard !Task.isCancelled else { return }
        place()
      }
      tasks.append(task)
      return task
    }
    if hasGeometry {
      selection.report("tile", at: first)
    }
    let stood = selection.place("tile") { frame in
      reports.append(frame)
      guard let anchor = CardScrollRoom.revealAnchor(row: frame, viewport: viewport, clearance: clearance) else {
        selection.cancelPlacement()
        return
      }
      selection.queue("tile", using: placement) { placements.append(anchor) }
    }
    #expect(stood == hasGeometry)
    if !hasGeometry {
      #expect(tasks.isEmpty)
      selection.report("tile", at: first)
    }
    #expect(tasks.count == 1)
    #expect(placements.isEmpty)
    selection.report("tile", at: taller)
    #expect(reports == [first, taller])
    #expect(tasks.count == 2)
    #expect(tasks.first?.isCancelled == true)
    let anchor = try #require(CardScrollRoom.revealAnchor(row: taller, viewport: viewport, clearance: clearance))
    #expect(anchor != CardScrollRoom.revealAnchor(row: first, viewport: viewport, clearance: clearance))
    for task in tasks { await task.value }
    #expect(placements == [anchor])

    let unfinished = taller.offsetBy(dx: 0, dy: -10)
    selection.report("tile", at: unfinished)
    #expect(reports == [first, taller, unfinished])
    #expect(tasks.count == 3)
    for task in tasks { await task.value }
    #expect(placements == [anchor, anchor])

    let arrived = CGRect(
      x: taller.minX,
      y: anchor.y * (viewport - taller.height),
      width: taller.width,
      height: taller.height
    )
    #expect(CardScrollRoom.revealAnchor(row: arrived, viewport: viewport, clearance: clearance) == nil)
    selection.report("tile", at: arrived)
    #expect(reports == [first, taller, unfinished, arrived])
    selection.report("tile", at: taller)
    #expect(reports == [first, taller, unfinished, arrived])
    #expect(tasks.count == 3)
    #expect(placements == [anchor, anchor])
  }

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
