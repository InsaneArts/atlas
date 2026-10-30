import AtlasService
import Foundation

/// Screenshot Copy, Save and Finder delivery, with one cancellable export at a time.
final class ViewerShotExports {
  /// The panes that receive completed work; never keeps the window alive.
  private weak var model: ViewerPanesModel?
  private let service: any ViewerService
  private let shotOutlet: ShotOutlet
  private var generation = 0
  /// The latest Copy or Save still reading its screenshot.
  private(set) var pending: Task<Void, Never>?

  /// Creates exports whose delivery ends when the window closes.
  ///
  /// - Parameters:
  ///   - model: The panes whose selection determines the export.
  ///   - service: The screenshot service.
  ///   - shotOutlet: Where pictures and file actions are delivered.
  init(model: ViewerPanesModel, service: any ViewerService, shotOutlet: ShotOutlet) {
    self.shotOutlet = shotOutlet
    self.model = model
    self.service = service
  }

  /// The screenshot Copy and Save act on: the one shown large, else the one selected; nil while none is.
  var actedShot: String? {
    guard let model, let viewer = model.viewer else { return nil }
    return model.panes.shots.large ?? ViewerPresenter.selectedShot(viewer.snapshot)
  }

  /// Copy: the large screenshot, else the selected one, goes to the clipboard as taken, read first if it has not
  /// arrived yet. Each Copy or Save overtakes any before it, so an older one still reading never lands over it.
  func copyShot() {
    withShot { [shotOutlet] png, _ in shotOutlet.copy(png) }
  }

  /// Save Screenshot…: the large screenshot, else the selected one, as taken, where the person picks in a save panel,
  /// read first if it has not arrived yet.
  func saveShot() {
    withShot { [weak self, shotOutlet] png, name in
      guard let self, let viewer = self.model?.viewer else { return }
      let file = ViewerPresenter.shotFileName(name, viewer.snapshot)
      if shotOutlet.save(png, file) == false { viewer.say(ViewerWords.shotNotSaved) }
    }
  }

  /// Stops a pending Copy or Save when the window closes, dropping its reply should it still come.
  func stopCopy() {
    generation += 1
    pending?.cancel()
    pending = nil
  }

  /// Runs `use` with the screenshot Copy and Save act on, as taken: at once when the large one has arrived, else once
  /// it is read. A newer Copy or Save overtakes it.
  ///
  /// - Parameter use: What to do with its PNG and its file name.
  private func withShot(_ use: @escaping @MainActor (Data, String) -> Void) {
    generation += 1
    pending?.cancel()
    pending = nil
    guard let model, let name = actedShot else { return }
    if name == model.panes.shots.large, let png = model.panes.shots.full?.png { return use(png, name) }
    let number = generation
    pending = Task { [weak self, service] in
      // A screenshot that cannot be read gives nothing; the person can ask again.
      let png = try? await service.screenshot(name: name, full: true)
      guard let self, let png, number == generation else { return }
      use(png, name)
    }
  }

  /// Show in Finder: the large screenshot's file, when it is listed and the service names its folder.
  func showShotInFinder() {
    guard let model, let name = model.panes.shots.large, let file = model.panes.shots.file(named: name) else { return }
    shotOutlet.reveal(file)
  }

}
