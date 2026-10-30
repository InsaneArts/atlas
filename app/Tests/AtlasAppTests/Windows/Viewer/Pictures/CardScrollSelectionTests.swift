import AppKit
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
    await CardScrollTests.settle(host)
    let end = try #require(list.documentView).frame.height - list.contentView.bounds.height
    let offset = list.contentView.bounds.minY
    #expect(
      offset > end - 16 && offset <= end,
      "reselected tile scrolled to \(offset) of \(end), previously \(revealed)"
    )
  }

  @Test("clearing the selection before its placement lands cancels the placement")
  func clearingTheSelectionCancelsItsPlacement() async throws {
    let selected = PreviewStates.viewerShotsMany
    let host = ViewerHost(selected, animatesArrivals: false, reduceMotion: true)
    let list = try #require(CardScrollTests.list(in: host))
    #expect(list.contentView.bounds.minY == 0)
    host.show(CardScrollTests.stepped(selected))
    // Building an offscreen tile may already scroll it to an edge; its final placement is still queued.
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
    await CardScrollTests.settle(host)
    #expect(
      abs(list.contentView.bounds.minY - beforePlacement) <= 1,
      "a cancelled placement moved the list after deselection"
    )
  }
}
