import AtlasService
import AtlasUI
import CoreGraphics
import Foundation

/// The action shown large and the task replayed over the Activity pane: what each shows, the pictures they read as
/// taken, Show changes' comparison, and a replay's playing. Each owns its work and cancels it; nothing is read or
/// played while the window is hidden or the Activity pane does not show.
final class ViewerActionViews {
  /// 1 s: how long a playing replay holds each frame.
  static let hold: Duration = .seconds(1)

  /// The panes that receive completed work; never keeps the window alive.
  private weak var model: ViewerPanesModel?
  private let service: any ViewerService
  /// The clock a playing replay waits on.
  private let clock: any Clock<Duration>
  /// The pictures being read as taken, by file name.
  private(set) var loads: [String: Task<Void, Never>] = [:]
  /// Show changes' comparison under way, by the pair it compares.
  private(set) var comparing: (key: String, task: Task<Void, Never>)?
  /// The replay playing.
  private(set) var playing: Task<Void, Never>?

  /// Creates the large view's and the replay's owner.
  ///
  /// - Parameters:
  ///   - model: The panes that receive each picture.
  ///   - service: The viewer service the pictures are read from.
  ///   - clock: The clock a playing replay waits on.
  init(model: ViewerPanesModel, service: any ViewerService, clock: any Clock<Duration>) {
    self.model = model
    self.service = service
    self.clock = clock
  }

  /// A step's picture pressed: its first action that kept a picture shows large, in its moment.
  ///
  /// - Parameter step: The step's identity.
  func open(_ step: String) {
    guard let model, let snapshot = model.viewer?.snapshot,
      let moments = ViewerPresenter.activityMoments(snapshot)
    else { return }
    for moment in moments.moments {
      guard let run = moment.runs.first(where: { $0.id == step }),
        let row = run.members.first(where: { $0.before != nil })
      else { continue }
      model.panes.activity.replay = nil
      model.panes.activity.action = ViewerActionFocus(moment: moment.id, row: row.id)
      return refresh()
    }
  }

  /// Previous or Next: the action `offset` along its moment, never past either end; how its pictures are compared
  /// stays.
  ///
  /// - Parameter offset: −1 for Previous, 1 for Next.
  func step(by offset: Int) {
    guard let model, let focus = model.panes.activity.action, let snapshot = model.viewer?.snapshot,
      let moment = ViewerPresenter.activityMoments(snapshot)?.moments.first(where: { $0.id == focus.moment })
    else { return }
    let sequence = ViewerPresenter.actionSequence(moment)
    guard let index = sequence.firstIndex(where: { $0.id == focus.row }),
      sequence.indices.contains(index + offset)
    else { return }
    model.panes.activity.action?.row = sequence[index + offset].id
    refresh()
  }

  /// Before, After or Slide.
  ///
  /// - Parameter mode: How the pictures are compared.
  func compare(_ mode: ViewerCompareMode) {
    model?.panes.activity.action?.compare = mode
  }

  /// Show changes: on, the two pictures are compared once both are read; off, the shade goes.
  func toggleChanges() {
    model?.panes.activity.action?.showsChanges.toggle()
    refresh()
  }

  /// Close, or Esc: the large view goes, and the pictures read for it.
  func close() {
    model?.panes.activity.action = nil
    refresh()
  }

  /// A moment's Replay: its first frame, paused.
  ///
  /// - Parameter moment: The moment's identity.
  func openReplay(_ moment: String) {
    guard let model else { return }
    model.panes.activity.action = nil
    model.panes.activity.replay = ViewerReplayFocus(moment: moment)
    refresh()
  }

  /// A frame picked: it shows, and a replay playing goes on from it.
  ///
  /// - Parameter index: The frame.
  func show(frame index: Int) {
    guard let model, model.panes.activity.replay != nil else { return }
    let last = frameCount - 1
    model.panes.activity.replay?.index = min(max(index, 0), last)
    refresh()
  }

  /// Previous or Next: the frame `offset` along, never past either end.
  ///
  /// - Parameter offset: −1 for Previous, 1 for Next.
  func step(frames offset: Int) {
    guard let index = model?.panes.activity.replay?.index else { return }
    show(frame: index + offset)
  }

  /// Play or Pause, or Space. Play from the last frame starts again from the first.
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

  /// Done, or Esc: the replay goes, and the pictures read for it.
  func closeReplay() {
    model?.panes.activity.replay = nil
    refresh()
  }

  /// Follows the window and the pane: focus whose action or replay is gone, as after its pictures were deleted or
  /// expired or its moment left the scope, is let go; shown on Activity, the pictures wanted are read and a replay
  /// plays on; else every reading and the playing stop, and resume when it shows again.
  func refresh() {
    guard let model else { return }
    if let snapshot = model.viewer?.snapshot {
      let shown = ViewerPresenter.shownOverlays(snapshot)
      if !shown.large { model.panes.activity.action = nil }
      if !shown.replay { model.panes.activity.replay = nil }
    }
    let shows = model.viewer?.shown == true && model.viewer?.session.place == .activity
    let wanted = shows ? wantedPictures() : []
    for (name, load) in loads where !wanted.contains(name) {
      load.cancel()
      loads[name] = nil
    }
    model.panes.activity.pictures = model.panes.activity.pictures.filter { wanted.contains($0.key) }
    for name in wanted where model.panes.activity.pictures[name] == nil && loads[name] == nil {
      read(name)
    }
    compareIfAsked(shows: shows)
    play(shows && model.panes.activity.replay?.isPlaying == true)
  }

  /// Stops every reading, the comparison and the playing, before the window is released.
  func stop() {
    for load in loads.values { load.cancel() }
    loads = [:]
    comparing?.task.cancel()
    comparing = nil
    playing?.cancel()
    playing = nil
  }

  /// How many frames the replay shows.
  private var frameCount: Int {
    guard let snapshot = model?.viewer?.snapshot, let focus = snapshot.panes.activity.replay,
      let moment = ViewerPresenter.activityMoments(snapshot)?.moments.first(where: { $0.id == focus.moment })
    else { return 0 }
    return ViewerPresenter.replayFrames(moment, snapshot).count
  }

  /// The pictures to read as taken: the action's before and after, or the replay's frame and the next.
  ///
  /// - Returns: Their file names.
  private func wantedPictures() -> Set<String> {
    guard let snapshot = model?.viewer?.snapshot, let moments = ViewerPresenter.activityMoments(snapshot) else {
      return []
    }
    let facts = snapshot.panes.activity
    if let focus = facts.action, let moment = moments.moments.first(where: { $0.id == focus.moment }),
      let entry = ViewerPresenter.actionSequence(moment).first(where: { $0.id == focus.row })
    {
      return Set([entry.before, entry.after].compactMap { $0 })
    }
    if let focus = facts.replay, let moment = moments.moments.first(where: { $0.id == focus.moment }) {
      let names = ViewerPresenter.replayFrames(moment, snapshot)
        .map(\.picture.name)
      return Set(names.dropFirst(focus.index).prefix(2))
    }
    return []
  }

  /// Reads a picture as taken and decodes it off the main actor; one no longer wanted when it lands is dropped, and
  /// one that cannot be read is tried again with the next change.
  ///
  /// - Parameter name: Its file name.
  private func read(_ name: String) {
    loads[name] = Task { [weak self, service] in
      let jpeg = try? await service.screenshot(name: name, full: true)
      var image: CGImage?
      if let jpeg { image = await ImageDecoding.image(jpeg) }
      guard !Task.isCancelled, let self, let model else { return }
      loads[name] = nil
      guard let image else { return }
      model.panes.activity.pictures[name] = DesktopPicture(id: "\(name) full", image: image)
      compareIfAsked(shows: true)
    }
  }

  /// Compares the action's two pictures once Show changes is on and both are read, off the main actor, once a pair.
  ///
  /// - Parameter shows: Whether the Activity pane shows.
  private func compareIfAsked(shows: Bool) {
    guard let model, shows, let focus = model.panes.activity.action, focus.showsChanges,
      let snapshot = model.viewer?.snapshot, let moments = ViewerPresenter.activityMoments(snapshot),
      let moment = moments.moments.first(where: { $0.id == focus.moment }),
      let entry = ViewerPresenter.actionSequence(moment).first(where: { $0.id == focus.row }),
      let before = entry.before, let after = entry.after,
      let first = model.panes.activity.pictures[before], let second = model.panes.activity.pictures[after]
    else {
      comparing?.task.cancel()
      comparing = nil
      return
    }
    let key = ViewerActivityFacts.changesKey(before: before, after: after)
    guard model.panes.activity.changes[key] == nil, comparing?.key != key else { return }
    comparing?.task.cancel()
    comparing = (
      key,
      Task { [weak self] in
        let squares = await PictureChanges.squares(before: first.image, after: second.image)
        guard !Task.isCancelled, let self, let model = self.model, comparing?.key == key else { return }
        comparing = nil
        model.panes.activity.changes = [key: squares]
      }
    )
  }

  /// Plays the replay, one frame every ``hold``, stopping on the last; or stops it.
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
