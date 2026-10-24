import AtlasService
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("ViewerPresenter+Shots")
struct ViewerPresenterShotsTests {
  @Test("opening a screenshot zooms it from its own tile")
  func openingZoomsFromItsOwnTile() throws {
    var snapshot = ViewerSnapshot()
    snapshot.panes.shots.shots = ["a.png", "b.png", "c.png"]
      .enumerated()
      .map { index, name in
        Screenshot(
          name: name,
          takenAt: snapshot.date.addingTimeInterval(Double(-60 * index)),
          width: 2560,
          height: 1440
        )
      }
    snapshot.panes.shots.large = "b.png"
    let shots = ViewerPresenter.shots(snapshot)
    let large = try #require(shots.large)
    // The large view and the tile it opened from carry one identity, which their picture's geometry shares; no other
    // tile carries it.
    let tiles = shots.days.flatMap(\.shots)
    #expect(large.id == "b.png")
    let carrying =
      tiles
      .map(\.id)
      .filter { $0 == large.id }
    #expect(carrying.count == 1)
  }
}
