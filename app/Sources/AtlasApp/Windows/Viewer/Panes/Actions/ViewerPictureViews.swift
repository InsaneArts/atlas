import AtlasService
import AtlasUI
import CoreGraphics
import Foundation

/// The picture sheet and the replay over the Activity pane: opening a screenshot or a step's picture, stepping
/// through its run's pictures, choosing Before or After, copying it and showing it in the Finder, and playing a run's
/// steps; reading the pictures each needs as taken, and dropping them once nothing shows them.
final class ViewerPictureViews {
  /// 1 s: how long the replay holds each frame while it plays.
  static let hold: Duration = .seconds(1)

  /// The panes it serves.
  private weak var model: ViewerPanesModel?
  /// Reads the pictures.
  private let service: any ViewerService
  /// Times the replay.
  private let clock: any Clock<Duration>
  /// Copies a picture and shows its file.
  private let outlet: ShotOutlet
  /// The pictures being read, by file name.
  private(set) var loads: [String: Task<Void, Never>] = [:]
  /// The replay playing, while it does.
  private(set) var playing: Task<Void, Never>?
  /// The picture being read to copy or save, while one is.
  private(set) var copying: Task<Void, Never>?

  /// Creates the views' work.
  ///
  /// - Parameters:
  ///   - model: The panes it serves.
  ///   - service: Reads the pictures.
  ///   - clock: Times the replay.
  ///   - outlet: Copies a picture and shows its file.
  init(model: ViewerPanesModel, service: any ViewerService, clock: any Clock<Duration>, outlet: ShotOutlet) {
    self.model = model
    self.service = service
    self.clock = clock
    self.outlet = outlet
  }

  /// Opens a screenshot in the sheet, stepping through its run's screenshots.
  ///
  /// - Parameter name: The screenshot's file name.
  func openShot(_ name: String) {
    guard let run = runs()?.first(where: { $0.runs.contains { $0.shot == name } }) else { return }
    show(ViewerPictureFocus(run: run.id, item: .shot(name), everyStep: false, opener: .shot(name)))
  }

  /// Opens the screen just before a step in the sheet, stepping through every step's picture in its run.
  ///
  /// - Parameter step: The step's identity.
  func openStep(_ step: String) {
    for run in runs() ?? [] {
      guard let line = run.runs.first(where: { $0.id == step }), let pictured = line.pictured else { continue }
      // The step is the opener, by its line's identity, which a repeat's picture may not share.
      let opener = ViewerPictureItem.before(line.id)
      return show(ViewerPictureFocus(run: run.id, item: .before(pictured.id), everyStep: true, opener: opener))
    }
  }

  /// ‹ or ›: the run's picture before or after, showing the screen before a step first.
  ///
  /// - Parameter offset: -1 for the one before, 1 for the one after.
  func step(by offset: Int) {
    guard let model, let focus = model.panes.activity.sheet,
      let run = runs()?.first(where: { $0.id == focus.run })
    else { return }
    let sequence = ViewerPresenter.pictureSequence(run, everyStep: focus.everyStep)
    let current = ViewerPresenter.pictureItem(focus.item, in: run)
    guard let index = sequence.firstIndex(of: current), sequence.indices.contains(index + offset) else { return }
    model.panes.activity.sheet?.item = sequence[index + offset]
    model.panes.activity.sheet?.side = .before
    refresh()
  }

  /// Before or After: which of a step's pictures the sheet shows.
  ///
  /// - Parameter side: The side.
  func side(_ side: ViewerPictureSide) {
    model?.panes.activity.sheet?.side = side
    refresh()
  }

  /// Done or Escape: closes the sheet, back where the picture was opened.
  func close() {
    copying?.cancel()
    copying = nil
    model?.panes.activity.sheet = nil
    refresh()
  }

  /// Copy: the sheet's picture, as taken, on the clipboard.
  func copy() {
    guard let name = shownPicture() else { return }
    copying?.cancel()
    copying = Task { [weak self, service] in
      // A picture that cannot be read gives nothing; the person can ask again.
      let data = try? await service.screenshot(name: name, full: true)
      let png = if let data { await ImageEncoding.png(data) } else { Data?.none }
      guard !Task.isCancelled, let self, let png else { return }
      copying = nil
      outlet.copy(png)
    }
  }

  /// Save: the sheet's picture, as taken, where the person chooses, named by when it was taken.
  ///
  /// - Parameter failed: Says the picture could not be saved.
  func save(failed: @escaping @MainActor () -> Void) {
    guard let name = shownPicture(), let snapshot = model?.viewer?.snapshot else { return }
    let file = ViewerPresenter.pictureFileName(name, snapshot)
    copying?.cancel()
    copying = Task { [weak self, service] in
      let data = try? await service.screenshot(name: name, full: true)
      let png = if let data { await ImageEncoding.png(data) } else { Data?.none }
      guard !Task.isCancelled, let self, let png else { return }
      copying = nil
      if outlet.save(png, file) == false { failed() }
    }
  }

  /// Show in Finder: the sheet's picture's file.
  func reveal() {
    guard let model, let name = shownPicture() else { return }
    let list = ScreenshotList(shots: model.panes.shots.shots ?? [], folder: model.panes.shots.folder)
    guard let file = list.pictureFile(named: name) else { return }
    outlet.reveal(file)
  }

  /// A run's Replay: its steps played over the window.
  ///
  /// - Parameter run: The run's identity.
  func openReplay(_ run: String) {
    guard let model else { return }
    model.panes.activity.sheet = nil
    model.panes.activity.replay = ViewerReplayFocus(moment: run)
    refresh()
  }

  /// A replay frame picked.
  ///
  /// - Parameter index: The frame.
  func show(frame index: Int) {
    guard let model, model.panes.activity.replay != nil else { return }
    let last = frameCount - 1
    model.panes.activity.replay?.index = min(max(index, 0), last)
    refresh()
  }

  /// ← or →: the replay's frame before or after.
  ///
  /// - Parameter offset: -1 for the one before, 1 for the one after.
  func step(frames offset: Int) {
    guard let index = model?.panes.activity.replay?.index else { return }
    show(frame: index + offset)
  }

  /// Play or Pause, from the first frame again once it has ended.
  func togglePlay() {
    guard let model, let replay = model.panes.activity.replay else { return }
    let last = frameCount - 1
    if replay.isPlaying {
      model.panes.activity.replay?.isPlaying = false
    } else {
      if replay.index >= last { model.panes.activity.replay?.index = 0 }
      model.panes.activity.replay?.isPlaying = true
    }
    refresh()
  }

  /// Done, or Escape: closes the replay.
  func closeReplay() {
    model?.panes.activity.replay = nil
    refresh()
  }

  /// Follows what shows: closes a sheet or a replay whose run is gone, reads the pictures they show and drops the
  /// rest, and plays or stops the replay.
  func refresh() {
    guard let model else { return }
    if let snapshot = model.viewer?.snapshot {
      let shown = ViewerPresenter.shownOverlays(snapshot)
      if !shown.sheet { model.panes.activity.sheet = nil }
      if !shown.replay { model.panes.activity.replay = nil }
    }
    let shows = model.viewer?.shown == true && model.viewer?.session.place == .activity
    if !shows || model.panes.activity.sheet == nil {
      // A copy or save asked of a sheet no longer shown never lands over what the person copies next.
      copying?.cancel()
      copying = nil
    }
    let wanted = shows ? wantedPictures() : []
    for (name, load) in loads where !wanted.contains(name) {
      load.cancel()
      loads[name] = nil
    }
    model.panes.activity.pictures = model.panes.activity.pictures.filter { wanted.contains($0.key) }
    for name in wanted where model.panes.activity.pictures[name] == nil && loads[name] == nil {
      read(name)
    }
    play(shows && model.panes.activity.replay?.isPlaying == true)
  }

  /// Stops reading pictures, copying and playing, as the window closes.
  func stop() {
    for load in loads.values { load.cancel() }
    loads = [:]
    copying?.cancel()
    copying = nil
    playing?.cancel()
    playing = nil
  }

  /// Shows a picture in the sheet, closing the replay.
  ///
  /// - Parameter focus: The picture.
  private func show(_ focus: ViewerPictureFocus) {
    guard let model else { return }
    model.panes.activity.replay = nil
    model.panes.activity.sheet = focus
    refresh()
  }

  /// Every run in scope, as the pane forms them.
  ///
  /// - Returns: The runs, or nil before the log is read.
  private func runs() -> [ActivityMoment]? {
    guard let snapshot = model?.viewer?.snapshot else { return nil }
    return ViewerPresenter.activityMoments(snapshot)?.moments
  }

  /// The file name of the picture the sheet shows.
  ///
  /// - Returns: The name, or nil while none shows.
  private func shownPicture() -> String? {
    guard let snapshot = model?.viewer?.snapshot, let moments = ViewerPresenter.activityMoments(snapshot) else {
      return nil
    }
    return ViewerPresenter.shownPicture(moments, snapshot)
  }

  /// How many frames the replay holds.
  private var frameCount: Int {
    guard let snapshot = model?.viewer?.snapshot, let focus = snapshot.panes.activity.replay,
      let moment = ViewerPresenter.activityMoments(snapshot)?.moment(focus.moment)
    else { return 0 }
    return ViewerPresenter.replayFrames(moment, snapshot).count
  }

  /// The pictures the sheet or the replay shows: a step's before and after, or a screenshot; the replay's frame and
  /// the next.
  ///
  /// - Returns: Their file names.
  private func wantedPictures() -> Set<String> {
    guard let snapshot = model?.viewer?.snapshot, let moments = ViewerPresenter.activityMoments(snapshot) else {
      return []
    }
    let facts = snapshot.panes.activity
    if let focus = facts.sheet, let run = moments.moment(focus.run) {
      switch focus.item {
      case .shot(let name):
        return [name]
      case .before(let row):
        let entry = run.runs
          .flatMap(\.members)
          .first { $0.id == row }
        return Set([entry?.before, entry?.after].compactMap { $0 })
      }
    }
    if let focus = facts.replay, let moment = moments.moment(focus.moment) {
      let names = ViewerPresenter.replayFrames(moment, snapshot)
        .map(\.picture.name)
      return Set(names.dropFirst(focus.index).prefix(2))
    }
    return []
  }

  /// Reads a picture as taken.
  ///
  /// - Parameter name: Its file name.
  private func read(_ name: String) {
    loads[name] = Task { [weak self, service] in
      let data = try? await service.screenshot(name: name, full: true)
      var image: CGImage?
      if let data { image = await ImageDecoding.image(data) }
      guard !Task.isCancelled, let self, let model else { return }
      loads[name] = nil
      guard let image else { return }
      model.panes.activity.pictures[name] = DesktopPicture(id: "\(name) full", image: image)
    }
  }

  /// Plays the replay, one frame each ``hold``, or stops it.
  ///
  /// - Parameter on: Whether it plays.
  private func play(_ on: Bool) {
    guard on else {
      playing?.cancel()
      playing = nil
      return
    }
    guard playing == nil else { return }
    playing = Task { [weak self, clock] in
      while !Task.isCancelled {
        // A cancelled wait ends at once, and the loop's condition ends the loop.
        try? await clock.sleep(for: Self.hold)
        guard !Task.isCancelled, let self, let model, let replay = model.panes.activity.replay, replay.isPlaying
        else { return }
        let last = frameCount - 1
        guard replay.index < last else {
          model.panes.activity.replay?.isPlaying = false
          playing = nil
          return
        }
        model.panes.activity.replay?.index = replay.index + 1
        refresh()
      }
    }
  }
}
