import AtlasService
import AtlasUI
import Foundation
import Observation

/// Coordinates the visible pane's readings and the host session history. Late replies are dropped by generation.
/// Activity reconciliation stays here; screenshot navigation, exports and thumbnails each own their work and
/// cancellation. The controller closes those tasks before releasing the window, and the owners hold it weakly.
@Observable
final class ViewerPanesModel {
  /// 2 s: Activity's pause between readings. An agent acts every second or two, so a new line shows about as soon as
  /// its caption does on the screen; the log is a file the service reads, which asks nothing of the helper.
  static let activityEvery: Duration = .seconds(2)
  /// 10 s: the host sessions' list's pause. The status says every few seconds which sessions are live and what each
  /// does; the list adds those that ended and what each made, which changes at an agent's pace, and is a few files the
  /// service reads.
  static let sessionsEvery: Duration = .seconds(10)
  /// The panes' facts, written by the reading coordinator and screenshot owners.
  var panes = ViewerPanes()

  /// The window these sidebar destinations belong to: whether it is on screen and the place it shows. Weak, as the
  /// window owns them.
  @ObservationIgnored weak var viewer: ViewerController?
  /// The viewer service it reads.
  @ObservationIgnored let service: any ViewerService
  /// The clock every wait runs on; a cancelled wait ends at once.
  @ObservationIgnored let clock: any Clock<Duration>
  /// Elapsed time for session refreshes, independent of the system date.
  @ObservationIgnored let uptime: @MainActor () -> TimeInterval
  /// The last history request, measured on the injected monotonic clock.
  @ObservationIgnored var sessionsReadAt: TimeInterval?
  /// The date and time now, which the panes tell times and days by.
  @ObservationIgnored let wallClock: @MainActor () -> Date
  /// The preceding ingested Activity runs; presenting snapshots never writes this history.
  @ObservationIgnored private var activityMoments: [ActivityMoment] = []
  /// The pane generation whose reading Activity last took in; nil before the first.
  @ObservationIgnored private var activityRead: Int?
  /// Whether the screenshots were listed with the reading Activity last took in.
  @ObservationIgnored private var shotsListed = false
  /// Where a copied picture goes, and how its file is shown.
  @ObservationIgnored private let shotOutlet: ShotOutlet
  /// The picture sheet and the run replayed over Activity: their pictures, Copy, Show in Finder and playing.
  @ObservationIgnored private(set) lazy var pictures = ViewerPictureViews(
    model: self,
    service: service,
    clock: clock,
    outlet: shotOutlet
  )
  /// The bounded thumbnail queue Activity's pictures share.
  @ObservationIgnored private(set) lazy var thumbnails = ViewerThumbnails(model: self, service: service)
  /// The pane and session readings under way.
  @ObservationIgnored var work = ViewerPaneWork()
  /// What the window does once the sessions' list is read, before the next reading is waited for.
  @ObservationIgnored let onSessionsRead: @MainActor (HostSessionList) async -> Void
  /// Where the apps' icons are found and kept.
  @ObservationIgnored let icons: AppIcons
  /// This window's pending icon deliveries, cancelled when its pane changes or the window closes.
  @ObservationIgnored var iconRequests: [String: Task<Void, Never>] = [:]

  /// Creates panes that have read nothing yet.
  ///
  /// - Parameters:
  ///   - viewer: The window they belong to.
  ///   - dependencies: What they reach outside the window.
  ///   - onSessionsRead: What the window does once the sessions' list is read.
  init(
    viewer: ViewerController,
    dependencies: ViewerDependencies,
    onSessionsRead: @escaping @MainActor (HostSessionList) async -> Void
  ) {
    self.viewer = viewer
    service = dependencies.service
    clock = dependencies.clock
    uptime = dependencies.uptime
    wallClock = dependencies.wallClock
    shotOutlet = dependencies.shotOutlet
    icons = dependencies.icons
    self.onSessionsRead = onSessionsRead
  }

  /// Reads Activity while it shows and the window is on screen, every ``activityEvery``, and stops otherwise. The
  /// thumbnails being read are let go unless Activity shows, and the pictures the sheet and the replay show follow the
  /// window. With the window gone, nothing shows, and the sessions' list is not read.
  func followPane() {
    followSessions()
    work.generation += 1
    for request in iconRequests.values { request.cancel() }
    iconRequests = [:]
    work.reading?.cancel()
    work.reading = nil
    let shown = viewer?.shown == true
    let place = viewer?.session.place ?? .screen
    thumbnails.follow(shown: shown, place: place)
    pictures.refresh()
    guard shown, place != .screen else { return }
    let number = work.generation
    work.reading = Task { [weak self, clock] in
      while !Task.isCancelled {
        guard self?.work.generation == number else { return }
        await self?.readActivity(number)
        guard self?.work.generation == number, !Task.isCancelled else { return }
        // A cancelled pause ends at once, and the loop's condition ends the loop.
        try? await clock.sleep(for: Self.activityEvery)
      }
    }
  }

  /// Applies one of Activity's events.
  ///
  /// - Parameter event: What the person did.
  func sendPane(_ event: ViewerEvent) {
    switch event {
    case .filterActivity(let filter): panes.activity.filter = filter
    case .toggleSession(let id, let opens): panes.activity.sessionChoices[id] = opens
    case .toggleRun(let id, let opens): panes.activity.choices[id] = opens
    case .showRunPictures(let id): panes.activity.pictureRuns[id] = false
    case .showAllRunPictures(let id): panes.activity.pictureRuns[id] = true
    case .hideRunPictures(let id): panes.activity.pictureRuns[id] = nil
    case .showDaySessions(let id): panes.activity.shownDays.insert(id)
    case .thumbnailNeeded(let name): thumbnails.thumbnailNeeded(name)
    case .openShot(let name): pictures.openShot(name)
    case .openAction(let step): pictures.openStep(step)
    case .previousPicture: pictures.step(by: -1)
    case .nextPicture: pictures.step(by: 1)
    case .pictureSide(let side): pictures.side(side)
    case .copyPicture: pictures.copy()
    case .showPictureInFinder: pictures.reveal()
    case .closePicture: pictures.close()
    case .openReplay(let run): pictures.openReplay(run)
    case .replayFrame(let index): pictures.show(frame: index)
    case .previousFrame: pictures.step(frames: -1)
    case .nextFrame: pictures.step(frames: 1)
    case .togglePlay: pictures.togglePlay()
    case .closeReplay: pictures.closeReplay()
    default: return
    }
  }

  /// Reads the log, and the screenshots while the log has a screenshot's row. A reply overtaken by a newer start or
  /// stop has no effect; a failure is kept until a reading succeeds, and a cancelled request says nothing. A later
  /// reading of the same log, while the pane shows, changes nothing (``unchanged(_:readAt:number:)``). The screenshots
  /// are listed again only with a changed log, while the picture sheet shows (its Show in Finder acts on their folder),
  /// or until a listing has worked and found every screenshot row's capture (``shotsFound(readAt:)``).
  ///
  /// - Parameter number: The reading's generation.
  func readActivity(_ number: Int) async {
    do {
      let log = try await service.activity()
      // Each row's age counts back from the moment the log arrived, never from after the screenshots' reply.
      let readAt = wallClock()
      // The screenshots' list sizes and finds the pictures: wanted while the log names a screenshot or a picture shows.
      let wantsShots =
        panes.activity.sheet != nil || log.entries.contains { $0.operation == ActivityGrouping.screenshotOperation }
      let lists =
        wantsShots
        && (panes.activity.sheet != nil || !unchanged(log, readAt: readAt, number: number)
          || !shotsFound(readAt: readAt))
      let list = lists ? try? await service.screenshots() : nil
      guard number == work.generation else { return }
      if lists { shotsListed = list != nil }
      if let list { keep(list) }
      if unchanged(log, readAt: readAt, number: number) {
        return followUnchanged(log)
      }
      activityRead = number
      panes.activity.log = log
      panes.activity.readAt = readAt
      let preparation = ViewerActivityPreparation(
        log: log,
        captures: panes.shots.shots ?? [],
        readAt: readAt,
        calendar: viewer?.snapshot.zonedCalendar ?? .current,
        now: readAt,
        preserving: activityMoments
      )
      activityMoments = preparation.moments.moments
      panes.activity.preparation = preparation
      panes.activity.failure = nil
      wantIcons(for: log.entries)
      // A newer reading may name the picture after a step in the sheet, or a replay's next frame.
      if panes.activity.sheet != nil || panes.activity.replay != nil { pictures.refresh() }
    } catch {
      guard number == work.generation, !error.isCancelled else { return }
      panes.activity.failure = error.message
    }
  }

  /// Whether a reading of `log` would change nothing the pane shows: the pane took in a reading of the very same log
  /// since it last showed, and the runs prepared from it stand on the same day, with the screenshots listed now. The
  /// first reading each time the pane shows is always taken in.
  ///
  /// - Parameters:
  ///   - log: The log just read.
  ///   - readAt: When it was read.
  ///   - number: The reading's generation.
  /// - Returns: Whether nothing would change.
  private func unchanged(_ log: ActivityLog, readAt: Date, number: Int) -> Bool {
    let facts = panes.activity
    guard number == work.generation, activityRead == number, let kept = facts.log, let keptAt = facts.readAt,
      let preparation = facts.preparation, kept.isSameReading(as: log)
    else { return false }
    return preparation.matches(
      log: kept,
      captures: panes.shots.shots ?? [],
      readAt: keptAt,
      calendar: viewer?.snapshot.zonedCalendar ?? .current,
      now: readAt
    )
  }

  /// Whether the last listing of the screenshots worked and found the capture of every screenshot row of the log the
  /// pane took in; until it does, a later listing may still hold what is missing.
  ///
  /// - Parameter readAt: When the log was read again.
  /// - Returns: Whether it did.
  private func shotsFound(readAt: Date) -> Bool {
    guard shotsListed, let kept = panes.activity.log, let keptAt = panes.activity.readAt else { return false }
    let calendar = viewer?.snapshot.zonedCalendar ?? .current
    return ActivityGrouping(log: kept, readAt: keptAt, now: readAt, calendar: calendar)
      .findsEveryShot(in: panes.shots.shots ?? [])
  }

  /// A reading of the log the pane already shows: nothing is taken in again, but what is still missing is asked for
  /// again, as after any reading: the icons not found, and the sheet's or the replay's pictures not read. A failure
  /// kept from before goes.
  ///
  /// - Parameter log: The log just read.
  private func followUnchanged(_ log: ActivityLog) {
    if panes.activity.failure != nil { panes.activity.failure = nil }
    wantIcons(for: log.entries)
    if panes.activity.sheet != nil || panes.activity.replay != nil { pictures.refresh() }
  }

  /// Keeps the screenshots' list: their names, sizes and folder.
  ///
  /// - Parameter list: The list.
  func keep(_ list: ScreenshotList) {
    if panes.shots.shots != list.shots { panes.shots.shots = list.shots }
    if panes.shots.folder != list.folder { panes.shots.folder = list.folder }
  }

  /// Stops reading the sheet's pictures and copying one, before the window is released.
  func stopPictures() { pictures.stop() }
}
