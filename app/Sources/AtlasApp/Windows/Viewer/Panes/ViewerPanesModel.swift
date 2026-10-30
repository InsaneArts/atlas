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
  /// 3 s: Apps' pause. Listing the apps asks the helper itself for its windows, more than a file read; apps open and
  /// windows move at the person's pace, and 3 s still shows a new window before they look for it.
  static let appsEvery: Duration = .seconds(3)
  /// 5 s: the Screenshots list's pause. An agent takes one every few seconds at most while it checks its work, and
  /// the list is a folder the service reads; 5 s shows a new one within a glance.
  static let shotsEvery: Duration = .seconds(5)
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
  /// The guarded read of every covering helper window, independent of the displayed apps.
  @ObservationIgnored private let appWindows: @MainActor () async -> HelperOccluderList?
  /// The clock every wait runs on; a cancelled wait ends at once.
  @ObservationIgnored let clock: any Clock<Duration>
  /// Elapsed time for session refreshes, independent of the system date.
  @ObservationIgnored let uptime: @MainActor () -> TimeInterval
  /// The last history request, measured on the injected monotonic clock.
  @ObservationIgnored var sessionsReadAt: TimeInterval?
  /// The date and time now, which the panes tell times and days by.
  @ObservationIgnored let wallClock: @MainActor () -> Date
  /// The preceding ingested Activity moments; presenting snapshots never writes this history.
  @ObservationIgnored private var activityMoments: [ActivityMoment] = []
  /// Where a copied screenshot goes, and how its file is shown.
  @ObservationIgnored private let shotOutlet: ShotOutlet
  /// Screenshot selection, row navigation and large-picture loading.
  @ObservationIgnored private(set) lazy var screenshots = ViewerShotNavigation(model: self, service: service)
  /// Cancellable Copy and Save delivery, and Finder actions.
  @ObservationIgnored private(set) lazy var exports = ViewerShotExports(
    model: self,
    service: service,
    shotOutlet: shotOutlet
  )
  /// The bounded thumbnail queue shared by Activity and Screenshots.
  @ObservationIgnored private(set) lazy var thumbnails = ViewerThumbnails(model: self, service: service)
  /// The pane and session readings under way.
  @ObservationIgnored var work = ViewerPaneWork()
  /// What the window does once the sessions' list is read, before the next reading is waited for.
  @ObservationIgnored let onSessionsRead: @MainActor (HostSessionList) async -> Void
  /// Where the apps' icons are found and kept.
  @ObservationIgnored let icons: AppIcons
  /// The app-name and bundle pairs already persisted by this viewer.
  @ObservationIgnored var iconWork = ViewerIconWork()
  /// This window's pending icon deliveries, cancelled when its pane changes or the window closes.
  @ObservationIgnored var iconRequests: [String: Task<Void, Never>] = [:]
  /// The latest evidence, including updates received before a request's task starts.
  @ObservationIgnored var iconWants: [String: AppIconWant] = [:]

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
    appWindows = dependencies.appWindows
    clock = dependencies.clock
    uptime = dependencies.uptime
    wallClock = dependencies.wallClock
    shotOutlet = dependencies.shotOutlet
    icons = dependencies.icons
    self.onSessionsRead = onSessionsRead
  }

  /// Reads the shown pane while the window is on screen, every its own pause, and stops reading any other. The
  /// thumbnails being read are let go unless Activity or Screenshots shows, and the large picture follows the window.
  /// With the window gone, nothing shows, and the sessions' list is not read.
  func followPane() {
    followSessions()
    work.generation += 1
    for request in iconRequests.values { request.cancel() }
    iconRequests = [:]
    iconWants = [:]
    work.reading?.cancel()
    work.reading = nil
    let shown = viewer?.shown == true
    let place = viewer?.session.place ?? .screen
    thumbnails.follow(shown: shown, place: place)
    screenshots.followLarge()
    guard shown, place != .screen else { return }
    let number = work.generation
    work.reading = Task { [weak self, clock] in
      while !Task.isCancelled {
        guard self?.work.generation == number else { return }
        await self?.read(place, number)
        guard self?.work.generation == number, !Task.isCancelled else { return }
        // A cancelled pause ends at once, and the loop's condition ends the loop.
        try? await clock.sleep(for: Self.pause(for: place))
      }
    }
  }

  /// The pause between a pane's readings.
  ///
  /// - Parameter place: The place.
  /// - Returns: Its pause.
  static func pause(for place: ViewerPlace) -> Duration {
    switch place {
    case .activity: activityEvery
    case .apps: appsEvery
    case .screen, .screenshots: shotsEvery
    }
  }

  /// One reading of a pane.
  ///
  /// - Parameters:
  ///   - place: The place.
  ///   - number: The reading's generation; a newer one drops its reply.
  func read(_ place: ViewerPlace, _ number: Int) async {
    switch place {
    case .activity: await readActivity(number)
    case .apps: await readApps(number)
    case .screenshots: await readShots(number)
    case .screen: return
    }
  }

  /// Applies one of the other panes' events.
  ///
  /// - Parameter event: What the person did.
  func sendPane(_ event: ViewerEvent) {
    switch event {
    case .filterActivity(let filter): panes.activity.filter = filter
    case .toggleMoment(let id, let opens): panes.activity.choices[id] = opens
    case .showMomentSteps(let id): panes.activity.expanded.insert(id)
    case .showWindow(let app, let window): showWindow(app: app, window: window)
    case .clearOutline: panes.apps.shown = nil
    case .shotColumns(let columns): if panes.shots.columns != columns { panes.shots.columns = columns }
    case .selectShot(let name): screenshots.select(name)
    case .openShot(let name): openShot(name)
    case .toggleShot: toggleShot()
    case .previousShot: stepShot(by: -1)
    case .nextShot: stepShot(by: 1)
    case .shotAbove: stepRow(by: -1)
    case .shotBelow: stepRow(by: 1)
    case .clearShotSelection: screenshots.clearSelection()
    case .closeShot: closeShot()
    case .copyShot: copyShot()
    case .showShotInFinder: showShotInFinder()
    case .thumbnailNeeded(let name): thumbnailNeeded(name)
    default: return
    }
  }

  /// Reads the log, and the screenshots while the log has a screenshot's row. A reply overtaken by a newer start or
  /// stop has no effect; a failure is kept to say only while nothing was read, and a cancelled request says nothing.
  ///
  /// - Parameter number: The reading's generation.
  func readActivity(_ number: Int) async {
    do {
      let log = try await service.activity()
      // Each row's age counts back from the moment the log arrived, never from after the screenshots' reply.
      let readAt = wallClock()
      let wantsShots = log.entries.contains { $0.operation == ActivityGrouping.screenshotOperation }
      let list = wantsShots ? try? await service.screenshots() : nil
      guard number == work.generation else { return }
      panes.activity.log = log
      panes.activity.readAt = readAt
      let preparation = ViewerActivityPreparation(
        log: log,
        captures: list?.shots ?? panes.shots.shots ?? [],
        readAt: readAt,
        calendar: viewer?.snapshot.zonedCalendar ?? .current,
        now: readAt,
        preserving: activityMoments
      )
      activityMoments = preparation.moments.moments
      panes.activity.preparation = preparation
      panes.activity.failure = nil
      wantIcons(for: log.entries)
      if let list { screenshots.keep(list) }
      // What the pane now shows is seen, also while the picture, and with it the status, is paused.
      viewer?.noteActivitySeen()
    } catch {
      guard number == work.generation, !error.isCancelled else { return }
      panes.activity.failure = error.message
    }
  }

  /// Reads the apps and wants the icons not found yet. A reply overtaken by a newer start or stop has no effect; a
  /// failure is kept to say only while nothing was read, and a cancelled request says nothing.
  ///
  /// - Parameter number: The reading's generation.
  func readApps(_ number: Int) async {
    do {
      let list = try await service.apps()
      let occluders = await appWindows()
      guard number == work.generation else { return }
      panes.apps.steadyWindows = Self.steadyWindows(list.apps, since: panes.apps.apps)
      panes.apps.apps = list.apps
      panes.apps.occluders = occluders
      panes.apps.sessions = list.sessions
      panes.apps.failure = nil
      wantIcons(for: list.apps)
    } catch {
      guard number == work.generation, !error.isCancelled else { return }
      panes.apps.failure = error.message
    }
  }

  /// The windows that stand where they stood in the reading before, at the same size, whose pictures may be cut
  /// from the live picture.
  ///
  /// - Parameters:
  ///   - apps: The apps just read.
  ///   - earlier: The apps the reading before listed; nil for none.
  /// - Returns: The steady windows' numbers.
  static func steadyWindows(_ apps: [HelperApp], since earlier: [HelperApp]?) -> Set<Int> {
    let before = Dictionary(
      (earlier ?? []).flatMap(\.windows)
        .map { ($0.id, $0) },
      uniquingKeysWith: { first, _ in first }
    )
    return Set(
      apps.flatMap(\.windows)
        .filter { window in before[window.id].map(window.standsAsBefore) ?? false }
        .map(\.id)
    )
  }

  /// Show on screen: outlines a window as it is now and shows the Screen pane. View-only: nothing goes to the helper.
  ///
  /// - Parameters:
  ///   - pid: The app's process id.
  ///   - id: The window's number.
  func showWindow(app pid: Int, window id: Int) {
    guard let app = panes.apps.apps?.first(where: { $0.pid == pid }),
      let window = app.windows.first(where: { $0.id == id })
    else { return }
    panes.apps.shown = ViewerShownWindow(app: app.name, bundleId: app.bundleId, window: window)
    viewer?.show(.screen)
  }

  /// Reads the screenshots and their folder. A reply overtaken by a newer start or stop has no effect; a failure is
  /// kept to say only while nothing was read, and a cancelled request says nothing.
  ///
  /// - Parameter number: The reading's generation.
  func readShots(_ number: Int) async {
    do {
      let list = try await service.screenshots()
      guard number == work.generation else { return }
      screenshots.keep(list)
      panes.shots.failure = nil
      screenshots.followLarge()
    } catch {
      guard number == work.generation, !error.isCancelled else { return }
      panes.shots.failure = error.message
    }
  }

  /// Screenshot operations keep their own task generations and cancellation.
  var actedShot: String? { exports.actedShot }
  /// Shows the named screenshot large, switching to Screenshots when needed.
  ///
  /// - Parameter name: The screenshot file name.
  func openShot(_ name: String) { screenshots.openShot(name) }
  /// Opens the selected screenshot large, or closes the large view.
  func toggleShot() { screenshots.toggleShot() }
  /// Moves the screenshot selection along the displayed order.
  ///
  /// - Parameter offset: The number of screenshots to move.
  func stepShot(by offset: Int) { screenshots.stepShot(by: offset) }
  /// Moves the screenshot selection by a displayed grid row.
  ///
  /// - Parameter offset: Minus one for up, one for down.
  func stepRow(by offset: Int) { screenshots.stepRow(by: offset) }
  /// Closes the large screenshot and cancels its pending load.
  func closeShot() { screenshots.closeShot() }
  /// Copies the acted-on screenshot as taken.
  func copyShot() { exports.copyShot() }
  /// Saves the acted-on screenshot where the person chooses.
  func saveShot() { exports.saveShot() }
  /// Cancels pending Copy or Save delivery before the window is released.
  func stopCopy() { exports.stopCopy() }
  /// Reveals the listed large screenshot in the folder named by the service.
  func showShotInFinder() { exports.showShotInFinder() }
  /// Queues a visible screenshot thumbnail unless it is already kept or pending.
  ///
  /// - Parameter name: The screenshot file name.
  func thumbnailNeeded(_ name: String) { thumbnails.thumbnailNeeded(name) }
  /// Holds new thumbnail reads during resizing, then resumes the queue.
  ///
  /// - Parameter held: Whether new reads wait.
  func holdThumbnails(_ held: Bool) { thumbnails.holdThumbnails(held) }
}
