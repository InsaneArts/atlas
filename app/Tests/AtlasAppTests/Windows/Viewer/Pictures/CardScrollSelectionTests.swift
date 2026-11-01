import AppKit
import AsyncSupportTesting
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("CardScroll selection lifecycle", .serialized, AppKitState())
@MainActor
struct CardScrollSelectionTests {
  @Test("a tile deselected, scrolled away from and selected again is revealed from where it stands now")
  func aReselectedTileIsRevealedFromItsCurrentPosition() async throws {
    let selected = PreviewStates.viewerShotsMany
    let host = ViewerHost(selected, animatesArrivals: false, reduceMotion: true)
    let list = try #require(CardScrollTests.list(in: host))
    let document = try #require(list.documentView)
    func isRevealed() -> Bool {
      let end = document.frame.height - list.contentView.bounds.height
      let offset = list.contentView.bounds.minY
      return offset > .zero && offset > end - Spacing.l && offset <= end
    }
    host.show(CardScrollTests.stepped(selected))
    try await Self.render(host, until: isRevealed)

    var cleared = selected
    var shots = try #require({ if case .screenshots(let shots) = cleared.pane { shots } else { nil } }())
    for day in shots.days.indices {
      for tile in shots.days[day].shots.indices {
        shots.days[day].shots[tile].isSelected = false
      }
    }
    cleared.pane = .screenshots(shots)
    host.show(cleared)
    try await Self.render(host, until: isRevealed)
    list.contentView.scroll(to: .zero)
    list.reflectScrolledClipView(list.contentView)
    try await Self.render(host) { list.contentView.bounds.minY == 0 }

    host.show(CardScrollTests.stepped(selected))
    try await Self.render(host, until: isRevealed)
  }

  @Test("clearing the selection before its placement lands cancels the placement")
  func clearingTheSelectionCancelsItsPlacement() async throws {
    let selected = PreviewStates.viewerShotsMany
    let held = HeldScrollPlacements()
    defer { held.cancel() }
    let host = ViewerHost(selected, animatesArrivals: false, reduceMotion: true, placement: held.placement)
    let list = try #require(CardScrollTests.list(in: host))
    #expect(list.contentView.bounds.minY == 0)
    host.show(CardScrollTests.stepped(selected))
    try host.render(until: "the selected tile has not queued its final placement") { !held.tasks.isEmpty }
    try await held.clock.untilSleeping(for: HeldScrollPlacements.delay, "the final placement is held after layout")
    // Building an offscreen tile may already scroll it to an edge. The final placement is provably pending.
    #expect(held.executions == 0)
    let beforePlacement = list.contentView.bounds.minY
    var cleared = selected
    var shots = try #require({ if case .screenshots(let shots) = cleared.pane { shots } else { nil } }())
    for day in shots.days.indices {
      for tile in shots.days[day].shots.indices {
        shots.days[day].shots[tile].isSelected = false
      }
    }
    shots.scrollRequest += 2
    cleared.pane = .screenshots(shots)
    host.show(cleared)
    #expect(list.contentView.bounds.minY == beforePlacement, "selection clears before the final placement lands")
    try host.render(until: "deselection has not cancelled the pending placement") {
      held.tasks.allSatisfy(\.isCancelled)
    }
    await held.finish()
    #expect(held.executions == 0, "the cancelled placement never scrolled the list after deselection")

    // A fresh selection still places the tile when released, proving that the held scheduler can execute real work.
    var reselected = selected
    var restored = try #require({ if case .screenshots(let shots) = selected.pane { shots } else { nil } }())
    restored.scrollRequest = shots.scrollRequest + 1
    reselected.pane = .screenshots(restored)
    host.show(reselected)
    try host.render(until: "reselection has not queued a fresh placement") { held.tasks.contains { !$0.isCancelled } }
    try await held.clock.untilSleeping(for: HeldScrollPlacements.delay, "the fresh placement waits for release")
    held.clock.advance(by: HeldScrollPlacements.delay)
    await held.finish()
    #expect(held.executions == 1, "the uncancelled placement executes on release")
  }

  @Test("a placement held across tile layout uses the tile's latest height")
  func aPlacementUsesTheLatestTileHeight() async throws {
    let selected = PreviewStates.viewerShotsMany
    let held = HeldScrollPlacements()
    defer { held.cancel() }
    var holding = true
    let placement = CardScrollPlacement { place in
      (holding ? held.placement : CardScrollPlacement())
        .schedule(place)
    }
    let host = ViewerHost(selected, animatesArrivals: false, reduceMotion: true, placement: placement)
    let list = try #require(CardScrollTests.list(in: host))
    let document = try #require(list.documentView)
    var updated = CardScrollTests.stepped(selected)
    host.show(updated)
    try host.render(until: "the selected tile has not queued its placement") { !held.tasks.isEmpty }
    try await held.clock.untilSleeping(for: HeldScrollPlacements.delay, "the placement waits for layout")
    let previousHeight = document.frame.height
    var shots = try #require({ if case .screenshots(let shots) = updated.pane { shots } else { nil } }())
    shots.heldColumns = 3
    updated.pane = .screenshots(shots)
    host.show(updated)
    try host.render(until: "the grid has not laid out its new column count") { document.frame.height != previousHeight }
    try await held.clock.untilSleeping(for: HeldScrollPlacements.delay, "the updated placement waits for release")
    holding = false
    held.clock.advance(by: HeldScrollPlacements.delay)
    try await Task { await held.finish() }
      .value("the released placement never finished")
    #expect(held.executions == 1)
    try await Self.render(host) {
      let end = document.frame.height - list.contentView.bounds.height
      let offset = list.contentView.bounds.minY
      return offset > end - Spacing.l && offset <= end
    }
    list.contentView.scroll(to: .zero)
    list.reflectScrolledClipView(list.contentView)
    try await Self.render(host) { list.contentView.bounds.minY == 0 }
  }

  private static func render(_ host: ViewerHost, until done: () -> Bool) async throws {
    let deadline = ContinuousClock.now
      .advanced(by: Ceiling.work)
    repeat {
      host.render()
      await afterQueuedWork()
    } while !done() && ContinuousClock.now < deadline
    try #require(done(), "the list never reached the expected scroll position")
  }
}
