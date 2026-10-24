import AsyncSupportTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("ViewerPanesModel grid")
struct ViewerPanesModelGridTests {
  /// Five screenshots, newest first, a second apart.
  static let names = (1...5)
    .map { "17926651\($0)0000000000-capture.png" }
    .reversed()
    .map { $0 }

  /// A viewer on the Screenshots tab over the five screenshots, two to a row.
  ///
  /// - Parameters:
  ///   - service: The scripted service.
  ///   - shotOutlet: Where copied and saved screenshots go.
  /// - Returns: The viewer.
  /// - Throws: The failed requirement, when the screenshots are never read.
  static func onGrid(
    _ service: ScriptedService,
    shotOutlet: ShotOutlet = ShotOutlet(copy: { _ in }, reveal: { _ in })
  ) async throws -> ViewerController {
    service.shots = names.enumerated()
      .map { index, name in Screenshot(name: name, takenAt: Date(timeIntervalSince1970: 1_792_665_150 - Double(index)))
      }
    let viewer = try await ViewerController.watching(service, shotOutlet: shotOutlet)
    viewer.send(.selectTab(.screenshots))
    try await settle("the screenshots were never read") { viewer.panes.shots.shots != nil }
    viewer.send(.shotColumns(2))
    return viewer
  }

  @Test("the up and down arrows move the selection a row of the grid, never past either end")
  func theArrowsMoveARow() async throws {
    let service = ScriptedService()
    let viewer = try await Self.onGrid(service)
    #expect(ViewerPresenter.selectedShot(viewer.snapshot) == Self.names[0], "the first is selected at first")
    viewer.send(.shotBelow)
    #expect(ViewerPresenter.selectedShot(viewer.snapshot) == Self.names[2])
    viewer.send(.shotBelow)
    #expect(ViewerPresenter.selectedShot(viewer.snapshot) == Self.names[4])
    viewer.send(.shotBelow)
    #expect(ViewerPresenter.selectedShot(viewer.snapshot) == Self.names[4], "no row below the last")
    viewer.send(.shotAbove)
    #expect(ViewerPresenter.selectedShot(viewer.snapshot) == Self.names[2])
    viewer.close()
  }

  @Test("Esc clears the selection, so nothing is ringed until one is picked or stepped to")
  func escapeClearsTheSelection() async throws {
    let service = ScriptedService()
    let viewer = try await Self.onGrid(service)
    viewer.send(.selectShot(Self.names[3]))
    viewer.send(.clearShotSelection)
    #expect(ViewerPresenter.selectedShot(viewer.snapshot) == nil)
    guard case .screenshots(let shots) = viewer.state.pane else {
      Issue.record("the Screenshots tab does not show")
      return
    }
    #expect(!shots.days.flatMap(\.shots).contains { $0.isSelected })
    viewer.send(.toggleShot)
    #expect(viewer.panes.shots.large == nil, "Space opens nothing with nothing selected")
    viewer.send(.nextShot)
    #expect(ViewerPresenter.selectedShot(viewer.snapshot) == Self.names[0], "an arrow selects the first again")
    viewer.close()
  }

  @Test("Copy and Save act on the selected screenshot as taken, without opening it large")
  func copyAndSaveTheSelectedScreenshot() async throws {
    let service = ScriptedService()
    service.shot = png(width: 3, height: 2)
    let copies = CallCount()
    let saves = CallCount()
    var saved: [String] = []
    let outlet = ShotOutlet(
      copy: { _ in copies.count += 1 },
      reveal: { _ in },
      save: { _, name in
        saved.append(name)
        saves.count += 1
        return true
      }
    )
    let viewer = try await Self.onGrid(service, shotOutlet: outlet)
    viewer.send(.selectShot(Self.names[1]))
    #expect(viewer.moreMenu.actsOnShot)
    viewer.send(.copyShot)
    try await settle("the screenshot was never copied") { copies.count == 1 }
    #expect(viewer.panes.shots.large == nil)
    viewer.panesModel.saveShot()
    try await settle("the screenshot was never saved") { saves.count == 1 }
    #expect(saved == [ViewerPresenter.shotFileName(Self.names[1], viewer.snapshot)])
    viewer.close()
  }

  @Test("Find puts the focus in the search of the tab shown, Activity's or Apps', and nowhere else")
  func findFocusesTheShownSearch() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    viewer.panesModel.find()
    #expect(viewer.panes.activity.findRequests == 0 && viewer.panes.apps.findRequests == 0, "the Screen has none")
    viewer.send(.selectTab(.activity))
    viewer.panesModel.find()
    #expect(viewer.panes.activity.findRequests == 1)
    viewer.send(.selectTab(.apps))
    viewer.panesModel.find()
    #expect(viewer.panes.apps.findRequests == 1)
    #expect(viewer.panes.activity.findRequests == 1, "a hidden tab's search is left be")
    viewer.close()
  }
}
