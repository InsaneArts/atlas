import AtlasService
import AtlasUI
import CoreGraphics
import Foundation
import Observation

/// The Activity, Apps and Screenshots panes: each reads only while it shows and the window is on screen, one reading
/// at a time on the injected clock, and a reply that arrives after its pane was left or the window hidden has no
/// effect. Activity reads the log, and the screenshots with it while a screenshot's row can show its capture. Apps
/// lists the helper's apps, and Show on screen outlines a window on the Screen pane and acts on nothing in the helper.
/// The apps' icons, which Activity's rows and Apps' apps both want, are found once each, a couple at a time off the
/// main actor (`ViewerPanesModel+Icons`). Screenshots has the selection, which the arrows move and Esc clears, the large view with
/// Previous and Next, Copy, Save and Show in Finder: Copy and Save take the picture as taken, of the large screenshot
/// or else the selected one, Save only where the person picks in a save panel, and Show in Finder shows only a listed
/// screenshot in the folder the service names. The screenshots' thumbnails, which Activity's screenshot lines
/// and the Screenshots grid share, are read through one queue a couple at a time as their tiles come into view,
/// decoded off the main actor, kept by name within a bound, and dropped when they come back after the queue was let
/// go. The host sessions' list is read while the window wants it, whichever pane shows. ``ViewerController`` owns one;
/// its state is written here alone.
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
  /// 2: thumbnails read at once. Each is a JPEG the service makes from a PNG of several megabytes, so two keep the
  /// grid filling steadily without crowding out the status and picture readings, which share the service's few
  /// request slots.
  static let thumbnailsAtOnce = 2

  /// What the Activity, Apps and Screenshots lists hold, written by this model alone, here and in its extensions.
  var panes = ViewerPanes()

  /// The window these sidebar destinations belong to: whether it is on screen and the place it shows. Weak, as the
  /// window owns them.
  @ObservationIgnored private weak var viewer: ViewerController?
  /// The viewer service it reads.
  @ObservationIgnored private let service: any ViewerService
  /// The clock every wait runs on; a cancelled wait ends at once.
  @ObservationIgnored private let clock: any Clock<Duration>
  /// The date and time now, which the panes tell times and days by.
  @ObservationIgnored private let wallClock: @MainActor () -> Date
  /// Where a copied screenshot goes, and how its file is shown.
  @ObservationIgnored private let shotOutlet: ShotOutlet
  /// The panes' reading and thumbnails under way.
  @ObservationIgnored private(set) var work = ViewerPaneWork()
  /// What the window does once the sessions' list is read, before the next reading is waited for.
  @ObservationIgnored private let onSessionsRead: @MainActor (HostSessionList) async -> Void
  /// Where the apps' icons are found and kept.
  @ObservationIgnored let icons: AppIcons
  /// The icon queue's working parts.
  @ObservationIgnored var iconWork = ViewerIconWork()

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
    work.reading?.cancel()
    work.reading = nil
    let shown = viewer?.shown == true
    let place = viewer?.session.place ?? .screen
    if !shown || (place != .activity && place != .screenshots) {
      resetThumbnails()
    } else {
      work.thumbnails.wanted = work.thumbnails.wanted.filter { name in
        panes.shots.shots?.contains(where: { $0.name == name }) == true && panes.thumbnails[name] == nil
      }
      readThumbnails()
    }
    followLarge()
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

  /// One reading of a tab.
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
    case .selectShot(let name): select(name)
    case .openShot(let name): openShot(name)
    case .toggleShot: toggleShot()
    case .previousShot: stepShot(by: -1)
    case .nextShot: stepShot(by: 1)
    case .shotAbove: stepRow(by: -1)
    case .shotBelow: stepRow(by: 1)
    case .clearShotSelection: clearSelection()
    case .closeShot: closeShot()
    case .copyShot: copyShot()
    case .showShotInFinder: showShotInFinder()
    case .thumbnailNeeded(let name): thumbnailNeeded(name)
    default: return
    }
  }

  /// Discovers ended sessions whenever the window is shown. Status readings refresh that history at most every
  /// ``sessionsEvery``; only multiple live sessions or a chosen session need a separate polling loop.
  /// A service that does not offer the list is asked again with a status reading once
  /// ``ViewerSessionsFacts/askAgainAfter`` has passed, as it may have been updated meanwhile.
  ///
  /// - Parameter readNow: Whether to read at once even while a reading loop runs, as when a session is chosen.
  func followSessions(readNow: Bool = false) {
    guard let viewer, viewer.shown else {
      work.sessionsReadAt = nil
      return stopSessions()
    }
    let now = wallClock()
    guard panes.sessions.asks(at: now)
    else { return stopSessions() }
    let polls = viewer.reading.statusReading.showsSessions || viewer.session.chosenSession != .all
    if work.sessions != nil {
      if !readNow, !work.sessionsPolling || polls {
        // An initial history request can become a live poll without asking twice before its first reply.
        work.sessionsPolling = polls
        return
      }
      stopSessions()
    }
    if !polls, !readNow, let readAt = work.sessionsReadAt,
      Duration.seconds(now.timeIntervalSince(readAt)) < Self.sessionsEvery
    {
      return
    }
    stopSessions()
    work.sessionsPolling = polls
    work.sessionsReadAt = now
    let number = work.sessionsGeneration
    work.sessions = Task { [weak self, clock] in
      while !Task.isCancelled {
        guard self?.work.sessionsGeneration == number else { return }
        await self?.readSessions(number)
        guard self?.work.sessionsGeneration == number, !Task.isCancelled else { return }
        guard self?.work.sessionsPolling == true else {
          self?.work.sessions = nil
          return
        }
        // A cancelled pause ends at once, and the loop's condition ends the loop.
        try? await clock.sleep(for: Self.sessionsEvery)
      }
    }
  }

  /// Stops reading the host sessions' list: its loop ends and a late reply is dropped.
  private func stopSessions() {
    work.sessionsGeneration += 1
    work.sessions?.cancel()
    work.sessions = nil
  }

  /// Reads the host sessions' list. A reply overtaken by a newer start or stop has no effect; a service that does not
  /// offer it is left until ``ViewerSessionsFacts/askAgainAfter`` has passed; another failure is kept, and a cancelled
  /// request says nothing. Once read, the window is told, so a cleanup whose reply never came is read again with it.
  ///
  /// - Parameter number: The reading's generation.
  func readSessions(_ number: Int) async {
    work.sessionsReadAt = wallClock()
    do {
      let list = try await service.sessions()
      guard number == work.sessionsGeneration else { return }
      panes.sessions.list = list
      panes.sessions.failure = nil
      panes.sessions.notOfferedAt = nil
      await onSessionsRead(list)
    } catch {
      guard number == work.sessionsGeneration, !error.isCancelled else { return }
      guard !error.isNotOffered else {
        panes.sessions.notOfferedAt = wallClock()
        panes.sessions.list = nil
        return stopSessions()
      }
      panes.sessions.failure = error.message
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
      let grouping = ActivityGrouping(
        log: log,
        readAt: readAt,
        now: readAt,
        calendar: viewer?.snapshot.zonedCalendar ?? .current
      )
      panes.activity.moments =
        ActivityMoments(
          grouping,
          captures: list?.shots ?? panes.shots.shots ?? [],
          scope: .all,
          preserving: panes.activity.moments
        )
        .moments
      panes.activity.log = log
      panes.activity.readAt = readAt
      panes.activity.failure = nil
      wantIcons(for: log.entries)
      if let list { keep(list) }
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
      guard number == work.generation else { return }
      panes.apps.steadyWindows = Self.steadyWindows(list.apps, since: panes.apps.apps)
      panes.apps.apps = list.apps
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

  /// Show on screen: outlines a window as it is now and shows the Screen tab. View-only: nothing goes to the helper.
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
      keep(list)
      panes.shots.failure = nil
      followLarge()
    } catch {
      guard number == work.generation, !error.isCancelled else { return }
      panes.shots.failure = error.message
    }
  }

  /// Keeps a reading of the screenshots and the folder they are in.
  ///
  /// - Parameter list: The reading.
  private func keep(_ list: ScreenshotList) {
    let names = Set(list.shots.map(\.name))
    if let large = panes.shots.large, !names.contains(large) { closeShot() }
    if let selected = panes.shots.selected, !names.contains(selected) { panes.shots.selected = nil }
    panes.shots.shots = list.shots
    panes.shots.folder = list.folder
  }

  /// Shows a screenshot large on the Screenshots tab, from its grid or an Activity line's Open, so Previous and Next
  /// go from it.
  ///
  /// - Parameter name: The screenshot's file name.
  func openShot(_ name: String) {
    guard let viewer else { return }
    if viewer.session.place != .screenshots { viewer.show(.screenshots) }
    select(name)
    showLarge(name)
  }

  /// Selects a screenshot, ringing it in the grid.
  ///
  /// - Parameter name: The screenshot's file name.
  private func select(_ name: String) {
    panes.shots.navigationColumn = nil
    panes.shots.selected = name
    panes.shots.selectionCleared = false
  }

  /// Esc with nothing shown large: no screenshot is selected until one is picked or stepped to.
  private func clearSelection() {
    panes.shots.navigationColumn = nil
    panes.shots.selected = nil
    panes.shots.selectionCleared = true
  }

  /// Space: closes the large view, or shows the selected screenshot large.
  func toggleShot() {
    if panes.shots.large != nil { return closeShot() }
    guard let viewer, let name = ViewerPresenter.selectedShot(viewer.snapshot) else { return }
    showLarge(name)
  }

  /// Previous or Next, ← or →, or a row up or down, ↑ or ↓: the screenshot `offset` places along in the grid's order,
  /// never past either end; from the large one while it shows, else from the selection, else the first is selected.
  ///
  /// - Parameter offset: −1 for Previous, 1 for Next, or a row's count either way.
  func stepShot(by offset: Int) {
    guard let viewer else { return }
    panes.shots.scrollRequest += 1
    let snapshot = viewer.snapshot
    let order = ViewerPresenter.shotOrder(snapshot)
    guard let from = panes.shots.large ?? ViewerPresenter.selectedShot(snapshot, order: order) else {
      if let first = order.first { select(first) }
      return
    }
    guard let next = ShotGrouping.step(from: from, by: offset, in: order) else { return }
    select(next)
    if panes.shots.large != nil { showLarge(next) }
  }

  /// Moves through the rows the grouped grids actually draw, retaining the preferred column across short rows.
  ///
  /// - Parameter offset: −1 for up, 1 for down.
  func stepRow(by offset: Int) {
    guard let viewer else { return }
    let columns = max(1, panes.shots.columns)
    let days = ViewerPresenter.shots(viewer.snapshot).days
    let rows = days.flatMap { day in
      stride(from: 0, to: day.shots.count, by: columns)
        .map { start in
          Array(day.shots[start..<min(start + columns, day.shots.count)])
            .map(\.id)
        }
    }
    panes.shots.scrollRequest += 1
    guard let from = panes.shots.large ?? ViewerPresenter.selectedShot(viewer.snapshot),
      let row = rows.firstIndex(where: { $0.contains(from) }),
      let column = rows[row].firstIndex(of: from)
    else {
      if let first = rows.first?.first { select(first) }
      return
    }
    guard rows.indices.contains(row + offset) else { return }
    let preferred = panes.shots.navigationColumn ?? column
    let nextRow = rows[row + offset]
    let next = nextRow[min(preferred, nextRow.count - 1)]
    select(next)
    panes.shots.navigationColumn = preferred
    if panes.shots.large != nil { showLarge(next) }
  }

  /// Close or Esc: the large view goes, and its picture with it.
  func closeShot() {
    stopLargeLoad()
    panes.shots.large = nil
    panes.shots.full = nil
  }

  /// The screenshot Copy and Save act on: the one shown large, else the one selected; nil while none is.
  var actedShot: String? {
    guard let viewer else { return nil }
    return panes.shots.large ?? ViewerPresenter.selectedShot(viewer.snapshot)
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
      guard let self, let viewer else { return }
      let file = ViewerPresenter.shotFileName(name, viewer.snapshot)
      if shotOutlet.save(png, file) == false { viewer.say(ViewerWords.shotNotSaved) }
    }
  }

  /// Stops a pending Copy or Save when the window closes, dropping its reply should it still come.
  func stopCopy() {
    work.large.copyGeneration += 1
    work.large.copy?.cancel()
    work.large.copy = nil
  }

  /// Runs `use` with the screenshot Copy and Save act on, as taken: at once when the large one has arrived, else once
  /// it is read. A newer Copy or Save overtakes it.
  ///
  /// - Parameter use: What to do with its PNG and its file name.
  private func withShot(_ use: @escaping @MainActor (Data, String) -> Void) {
    work.large.copyGeneration += 1
    work.large.copy?.cancel()
    work.large.copy = nil
    guard let name = actedShot else { return }
    if name == panes.shots.large, let png = panes.shots.full?.png { return use(png, name) }
    let number = work.large.copyGeneration
    work.large.copy = Task { [weak self, service] in
      // A screenshot that cannot be read gives nothing; the person can ask again.
      let png = try? await service.screenshot(name: name, full: true)
      guard let self, let png, number == work.large.copyGeneration else { return }
      use(png, name)
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
  private func followLarge() {
    if viewer?.shown != true || viewer?.session.place != .screenshots {
      stopLargeLoad()
    } else if let name = panes.shots.large, panes.shots.full == nil, work.large.load == nil {
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
    work.large.generation += 1
    let number = work.large.generation
    work.large.load = Task { [weak self, service] in
      // A screenshot that cannot be read shows none, and is tried again with the next reading.
      let png = try? await service.screenshot(name: name, full: true)
      var image: CGImage?
      if let png { image = await ImageDecoding.image(png) }
      guard let self, number == work.large.generation else { return }
      // Ended either way, so a read that failed is tried again with the next reading of the screenshots.
      work.large.load = nil
      guard let png, panes.shots.large == name else { return }
      panes.shots.full = ViewerFullShot(png: png, picture: image.map { DesktopPicture(id: "\(name) full", image: $0) })
    }
  }

  /// Stops the large picture being read: cancelled, and its reply dropped should it still come.
  private func stopLargeLoad() {
    work.large.generation += 1
    work.large.load?.cancel()
    work.large.load = nil
  }

  /// A thumbnail came into view: read it, unless it is kept, being read or waiting already.
  ///
  /// - Parameter name: The screenshot's file name.
  func thumbnailNeeded(_ name: String) {
    guard panes.thumbnails[name] == nil, work.thumbnails.loading[name] == nil,
      !work.thumbnails.wanted.contains(name)
    else { return }
    work.thumbnails.wanted.append(name)
    readThumbnails()
  }

  /// Holds new thumbnail reads while the window is being resized, so tiles coming into view as it changes wait in turn
  /// rather than read in a burst; once it ends, those waiting are read.
  ///
  /// - Parameter held: Whether new reads wait.
  func holdThumbnails(_ held: Bool) {
    guard work.thumbnails.isHeld != held else { return }
    work.thumbnails.isHeld = held
    if !held { readThumbnails() }
  }

  /// Starts the waiting thumbnails, oldest first, while fewer than ``thumbnailsAtOnce`` are being read, unless reads
  /// are held.
  private func readThumbnails() {
    while !work.thumbnails.isHeld, work.thumbnails.loading.count < Self.thumbnailsAtOnce,
      !work.thumbnails.wanted.isEmpty
    {
      let name = work.thumbnails.wanted.removeFirst()
      let number = work.thumbnails.generation
      work.thumbnails.loading[name] = Task { [weak self, service] in
        var image: CGImage?
        // A thumbnail that cannot be read lands as none, its tile left as it was.
        if let jpeg = try? await service.screenshot(name: name, full: false) {
          image = await ImageDecoding.image(jpeg)
        }
        self?.land(image, for: name, number: number)
      }
    }
  }

  /// A thumbnail read: kept, unless the queue was let go since it was asked for, and the next one starts.
  ///
  /// - Parameters:
  ///   - image: The thumbnail, or nil when it could not be read.
  ///   - name: The screenshot's file name.
  ///   - number: The queue's generation when it was asked for.
  func land(_ image: CGImage?, for name: String, number: Int) {
    guard number == work.thumbnails.generation else { return }
    work.thumbnails.loading[name] = nil
    if let image { panes.thumbnails.keep(DesktopPicture(id: name, image: image), for: name) }
    readThumbnails()
  }

  /// Pauses the queue: what is being read is cancelled and waits with the rest for the pane to show again. Any reply
  /// still to come is dropped. What is kept stays.
  func resetThumbnails() {
    work.thumbnails.generation += 1
    work.thumbnails.wanted.insert(contentsOf: work.thumbnails.loading.keys, at: 0)
    for load in work.thumbnails.loading.values { load.cancel() }
    work.thumbnails.loading.removeAll()
  }
}
