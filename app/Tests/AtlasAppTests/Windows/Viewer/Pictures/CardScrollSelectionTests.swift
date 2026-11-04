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
    func position() -> String {
      "offset \(list.contentView.bounds.minY) of \(document.frame.height - list.contentView.bounds.height)"
    }
    host.show(CardScrollTests.stepped(selected))
    try await Self.render(host, "the first selection was not revealed", position, until: isRevealed)

    var cleared = selected
    var shots = try #require({ if case .screenshots(let shots) = cleared.pane { shots } else { nil } }())
    for day in shots.days.indices {
      for tile in shots.days[day].shots.indices {
        shots.days[day].shots[tile].isSelected = false
      }
    }
    cleared.pane = .screenshots(shots)
    host.show(cleared)
    try await Self.render(host, "deselection moved the list", position, until: isRevealed)
    list.contentView.scroll(to: .zero)
    list.reflectScrolledClipView(list.contentView)
    try await Self.render(host, "the list did not scroll to its top", position) {
      list.contentView.bounds.minY == 0
    }
    // AppKit's offset changes at once; SwiftUI learns of the scroll in its next passes. Until it has, it still
    // takes the list to stand at the tile, and a reveal asked for then can be dropped as already done.
    await Self.settle(host)

    host.show(CardScrollTests.stepped(selected))
    try await Self.render(host, "the reselected tile was not revealed", position, until: isRevealed)
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

  /// Renders until `done`, failing with `failure` and where the list stands if the ceiling passes first.
  private static func render(
    _ host: ViewerHost,
    _ failure: String,
    _ position: () -> String,
    until done: () -> Bool
  ) async throws {
    let deadline = ContinuousClock.now
      .advanced(by: Ceiling.work)
    repeat {
      host.render()
      await afterQueuedWork()
    } while !done() && ContinuousClock.now < deadline
    try #require(done(), "\(failure): \(position())")
  }

  /// A few rendering passes with the main actor's queued work between them, so SwiftUI has seen what AppKit did.
  private static func settle(_ host: ViewerHost) async {
    for _ in 0..<6 {
      await afterQueuedWork()
      host.render()
    }
  }
}
