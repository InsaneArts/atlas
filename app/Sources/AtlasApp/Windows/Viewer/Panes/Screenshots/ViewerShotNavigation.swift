import AtlasService
import AtlasUI
import CoreGraphics
import Foundation

/// Screenshot navigation and the large picture's load and cancellation.
final class ViewerShotNavigation {
  /// The panes that receive completed work; never keeps the window alive.
  private weak var model: ViewerPanesModel?
  private let service: any ViewerService
  /// Only this owner starts and stops the large picture.
  private var generation = 0
  /// The large picture being read, cancelled before the window is released.
  private(set) var load: Task<Void, Never>?

  /// Creates navigation for the panes, reading full screenshots through the service.
  ///
  /// - Parameters:
  ///   - model: The panes that receive each picture.
  ///   - service: The screenshot service.
  init(model: ViewerPanesModel, service: any ViewerService) {
    self.model = model
    self.service = service
  }

  /// Keeps a reading of the screenshots and the folder they are in.
  ///
  /// - Parameter list: The reading.
  func keep(_ list: ScreenshotList) {
    guard let model else { return }
    let names = Set(list.shots.map(\.name))
    if let large = model.panes.shots.large, !names.contains(large) { closeShot() }
    if let selected = model.panes.shots.selected, !names.contains(selected) { model.panes.shots.selected = nil }
    model.panes.shots.shots = list.shots
    model.panes.shots.folder = list.folder
  }

  /// Shows a screenshot large on the Screenshots pane, from its grid or an Activity line's Open, so Previous and Next
  /// go from it.
  ///
  /// - Parameter name: The screenshot's file name.
  func openShot(_ name: String) {
    guard let model else { return }
    guard let viewer = model.viewer else { return }
    if viewer.session.place != .screenshots { viewer.show(.screenshots) }
    select(name)
    showLarge(name)
  }

  /// Selects a screenshot, ringing it in the grid.
  ///
  /// - Parameter name: The screenshot's file name.
  func select(_ name: String) {
    guard let model else { return }
    model.panes.shots.navigationColumn = nil
    model.panes.shots.selected = name
    model.panes.shots.selectionCleared = false
  }

  /// Esc with nothing shown large: no screenshot is selected until one is picked or stepped to.
  func clearSelection() {
    guard let model else { return }
    model.panes.shots.navigationColumn = nil
    model.panes.shots.selected = nil
    model.panes.shots.selectionCleared = true
  }

  /// Space: closes the large view, or shows the selected screenshot large.
  func toggleShot() {
    guard let model else { return }
    if model.panes.shots.large != nil { return closeShot() }
    guard let viewer = model.viewer, let name = ViewerPresenter.selectedShot(viewer.snapshot) else { return }
    showLarge(name)
  }

  /// Previous or Next, ← or →, or a row up or down, ↑ or ↓: the screenshot `offset` places along in the grid's order,
  /// never past either end; from the large one while it shows, else from the selection, else the first is selected.
  ///
  /// - Parameter offset: −1 for Previous, 1 for Next, or a row's count either way.
  func stepShot(by offset: Int) {
    guard let model else { return }
    guard let viewer = model.viewer else { return }
    model.panes.shots.scrollRequest += 1
    let snapshot = viewer.snapshot
    let order = ViewerPresenter.shotOrder(snapshot)
    guard let from = model.panes.shots.large ?? ViewerPresenter.selectedShot(snapshot, order: order) else {
      if let first = order.first { select(first) }
      return
    }
    guard let next = ShotGrouping.step(from: from, by: offset, in: order) else { return }
    select(next)
    if model.panes.shots.large != nil { showLarge(next) }
  }

  /// Moves through the rows the grouped grids actually draw, retaining the preferred column across short rows.
  ///
  /// - Parameter offset: −1 for up, 1 for down.
  func stepRow(by offset: Int) {
    guard let model else { return }
    guard let viewer = model.viewer else { return }
    let columns = max(1, model.panes.shots.columns)
    let days = ViewerPresenter.shots(viewer.snapshot).days
    let rows = days.flatMap { day in
      stride(from: 0, to: day.shots.count, by: columns)
        .map { start in
          Array(day.shots[start..<min(start + columns, day.shots.count)])
            .map(\.id)
        }
    }
    model.panes.shots.scrollRequest += 1
    guard let from = model.panes.shots.large ?? ViewerPresenter.selectedShot(viewer.snapshot),
      let row = rows.firstIndex(where: { $0.contains(from) }),
      let column = rows[row].firstIndex(of: from)
    else {
      if let first = rows.first?.first { select(first) }
      return
    }
    guard rows.indices.contains(row + offset) else { return }
    let preferred = model.panes.shots.navigationColumn ?? column
    let nextRow = rows[row + offset]
    let next = nextRow[min(preferred, nextRow.count - 1)]
    select(next)
    model.panes.shots.navigationColumn = preferred
    if model.panes.shots.large != nil { showLarge(next) }
  }

  /// Close or Esc: the large view goes, and its picture with it.
  func closeShot() {
    guard let model else { return }
    stopLargeLoad()
    model.panes.shots.large = nil
    model.panes.shots.full = nil
  }

  /// Follows the window on and off screen: hidden or closed, the large picture still reading stops and never lands;
  /// on screen again, or with each reading of the screenshots, a screenshot still large and not yet read is read
  /// again.
  func followLarge() {
    guard let model else { return }
    if model.viewer?.shown != true || model.viewer?.session.place != .screenshots {
      stopLargeLoad()
    } else if let name = model.panes.shots.large, model.panes.shots.full == nil, load == nil {
      loadLarge(name)
    }
  }

  /// Shows `name` large: its thumbnail at once, then the picture as taken.
  ///
  /// - Parameter name: The screenshot's file name.
  private func showLarge(_ name: String) {
    guard let model else { return }
    closeShot()
    model.panes.shots.large = name
    model.thumbnailNeeded(name)
    loadLarge(name)
  }

  /// Reads `name` as taken and decodes it off the main actor; a picture read before a newer one was asked for, or
  /// before the window hid, never lands.
  ///
  /// - Parameter name: The screenshot's file name.
  private func loadLarge(_ name: String) {
    generation += 1
    let number = generation
    load = Task { [weak self, service] in
      // A screenshot that cannot be read shows none, and is tried again with the next reading.
      let png = try? await service.screenshot(name: name, full: true)
      var image: CGImage?
      if let png { image = await ImageDecoding.image(png) }
      guard let self, let model = self.model, number == generation else { return }
      // Ended either way, so a read that failed is tried again with the next reading of the screenshots.
      load = nil
      guard let png, model.panes.shots.large == name else { return }
      model.panes.shots.full = ViewerFullShot(
        png: png,
        picture: image.map { DesktopPicture(id: "\(name) full", image: $0) }
      )
    }
  }

  /// Stops the large picture being read: cancelled, and its reply dropped should it still come.
  private func stopLargeLoad() {
    generation += 1
    load?.cancel()
    load = nil
  }

}
