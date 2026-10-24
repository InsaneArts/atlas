import AtlasService
import AtlasUI
import CoreGraphics

/// The Screenshots tab: its reading, the selection, the large view with Previous and Next, Copy and Show in Finder.
/// Copy puts the picture as taken on the clipboard, and Show in Finder shows only a listed screenshot in the folder
/// the service names; nothing is ever saved.
extension ViewerController {
  /// Reads the screenshots and their folder. A reply overtaken by a newer start or stop has no effect; a failure is
  /// kept to say only while nothing was read, and a cancelled request says nothing.
  ///
  /// - Parameter number: The reading's generation.
  func readShots(_ number: Int) async {
    do {
      let list = try await service.screenshots()
      guard number == paneWork.generation else { return }
      keep(list)
      panes.shots.failure = nil
      followLarge()
    } catch {
      guard number == paneWork.generation, !error.isCancelled else { return }
      panes.shots.failure = error.message
    }
  }

  /// Keeps a reading of the screenshots and the folder they are in.
  ///
  /// - Parameter list: The reading.
  func keep(_ list: ScreenshotList) {
    panes.shots.shots = list.shots
    panes.shots.folder = list.folder
  }

  /// Shows a screenshot large on the Screenshots tab, from its grid or an Activity line's Open; a search that hides
  /// it gives way, so Previous and Next go from it.
  ///
  /// - Parameter name: The screenshot's file name.
  func openShot(_ name: String) {
    if session.tab != .screenshots { selectTab(.screenshots) }
    panes.shots.selected = name
    if !ViewerPresenter.shotOrder(snapshot).contains(name) { panes.shots.query = "" }
    showLarge(name)
  }

  /// Space: closes the large view, or shows the selected screenshot large.
  func toggleShot() {
    if panes.shots.large != nil { return closeShot() }
    guard let name = ViewerPresenter.selectedShot(snapshot) else { return }
    showLarge(name)
  }

  /// Previous or Next, ← or →: the screenshot `offset` places along in the grid's order, never past either end; the
  /// large one while it shows, else the selection.
  ///
  /// - Parameter offset: −1 for Previous, 1 for Next.
  func stepShot(by offset: Int) {
    let order = ViewerPresenter.shotOrder(snapshot)
    guard let from = panes.shots.large ?? ViewerPresenter.selectedShot(snapshot),
      let next = ShotGrouping.step(from: from, by: offset, in: order)
    else { return }
    panes.shots.selected = next
    if panes.shots.large != nil { showLarge(next) }
  }

  /// Close or Esc: the large view goes, and its picture with it.
  func closeShot() {
    stopLargeLoad()
    panes.shots.large = nil
    panes.shots.full = nil
  }

  /// Copy: the large screenshot as taken goes to the clipboard, read first if it has not arrived yet. Each Copy
  /// overtakes any before it, so an older one still reading never lands over it.
  func copyShot() {
    paneWork.large.copyGeneration += 1
    paneWork.large.copy?.cancel()
    paneWork.large.copy = nil
    if let png = panes.shots.full?.png { return shotOutlet.copy(png) }
    guard let name = panes.shots.large else { return }
    let number = paneWork.large.copyGeneration
    paneWork.large.copy = Task { [weak self, service] in
      let png = try? await service.screenshot(name: name, full: true)
      guard let self, let png, number == paneWork.large.copyGeneration else { return }
      shotOutlet.copy(png)
    }
  }

  /// Show in Finder: the large screenshot's file, when it is listed and the service names its folder.
  func showShotInFinder() {
    guard let name = panes.shots.large, let file = panes.shots.file(named: name) else { return }
    shotOutlet.reveal(file)
  }

  /// Follows the window on and off screen: hidden or closed, the large picture still reading stops and never lands;
  /// on screen again, or with each reading of the screenshots, a screenshot still large and not yet read is read
  /// again.
  func followLarge() {
    if !shown || session.tab != .screenshots {
      stopLargeLoad()
    } else if let name = panes.shots.large, panes.shots.full == nil, paneWork.large.load == nil {
      loadLarge(name)
    }
  }

  /// Shows `name` large: its thumbnail at once, then the picture as taken.
  ///
  /// - Parameter name: The screenshot's file name.
  private func showLarge(_ name: String) {
    closeShot()
    panes.shots.large = name
    thumbnailNeeded(name)
    loadLarge(name)
  }

  /// Reads `name` as taken and decodes it off the main actor; a picture read before a newer one was asked for, or
  /// before the window hid, never lands.
  ///
  /// - Parameter name: The screenshot's file name.
  private func loadLarge(_ name: String) {
    paneWork.large.generation += 1
    let number = paneWork.large.generation
    paneWork.large.load = Task { [weak self, service] in
      let png = try? await service.screenshot(name: name, full: true)
      var image: CGImage?
      if let png { image = await ImageDecoding.image(png) }
      guard let self, number == paneWork.large.generation else { return }
      // Ended either way, so a read that failed is tried again with the next reading of the screenshots.
      paneWork.large.load = nil
      guard let png, panes.shots.large == name else { return }
      panes.shots.full = ViewerFullShot(png: png, picture: image.map { DesktopPicture(id: "\(name) full", image: $0) })
    }
  }

  /// Stops the large picture being read: cancelled, and its reply dropped should it still come.
  private func stopLargeLoad() {
    paneWork.large.generation += 1
    paneWork.large.load?.cancel()
    paneWork.large.load = nil
  }
}
