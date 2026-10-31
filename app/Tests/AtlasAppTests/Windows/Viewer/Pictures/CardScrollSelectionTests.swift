import AppKit
import AsyncSupportTesting
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("CardScroll selection lifecycle", .serialized, AppKitState())
@MainActor
struct CardScrollSelectionTests {
  @Test("a tile deselected, scrolled away from and selected again is revealed from where it stands now")
  func aReselectedTileIsRevealedFromItsCurrentPosition() async throws {
    let selected = PreviewStates.viewerShotsMany
    let host = ViewerHost(selected, animatesArrivals: false, reduceMotion: true)
    let list = try #require(CardScrollTests.list(in: host))
    host.show(CardScrollTests.stepped(selected))
    await CardScrollTests.settle(host)
    let revealed = list.contentView.bounds.minY
    #expect(revealed > 0)

    var cleared = selected
    var shots = try #require({ if case .screenshots(let shots) = cleared.pane { shots } else { nil } }())
    for day in shots.days.indices {
      for tile in shots.days[day].shots.indices {
        shots.days[day].shots[tile].isSelected = false
      }
    }
    cleared.pane = .screenshots(shots)
    host.show(cleared)
    await CardScrollTests.settle(host)
    list.contentView.scroll(to: .zero)
    list.reflectScrolledClipView(list.contentView)
    await CardScrollTests.settle(host)
    #expect(list.contentView.bounds.minY == 0)

    host.show(CardScrollTests.stepped(selected))
    let deadline = ContinuousClock.now
      .advanced(by: Ceiling.work)
    var end = try #require(list.documentView).frame.height - list.contentView.bounds.height
    var offset = list.contentView.bounds.minY
    while !(offset > end - 16 && offset <= end), ContinuousClock.now < deadline {
      await CardScrollTests.settle(host)
      end = try #require(list.documentView).frame.height - list.contentView.bounds.height
      offset = list.contentView.bounds.minY
    }
    #expect(
      offset > end - 16 && offset <= end,
      "reselected tile scrolled to \(offset) of \(end), previously \(revealed)"
    )
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
}
