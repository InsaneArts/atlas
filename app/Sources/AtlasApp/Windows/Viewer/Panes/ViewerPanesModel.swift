import AtlasService
import AtlasUI
import CoreGraphics
import Foundation
import Observation

/// The Activity, Apps and Screenshots tabs: each reads only while it shows and the window is on screen, one reading
/// at a time on the injected clock, and a reply that arrives after its tab was left or the window hidden has no
/// effect. Activity reads the log, and the screenshots with it while a screenshot's row can show its capture. Apps
/// decodes each app's icon once off the main actor, and Show on screen outlines a window on the Screen tab and acts on
/// nothing in the helper. Screenshots has the selection, the large view with Previous and Next, Copy and Show in
/// Finder: Copy puts the picture as taken on the clipboard, and Show in Finder shows only a listed screenshot in the
/// folder the service names; nothing is ever saved. The screenshots' thumbnails, which Activity's screenshot lines
/// and the Screenshots grid share, are read through one queue a couple at a time as their tiles come into view,
/// decoded off the main actor, kept by name within a bound, and dropped when they come back after the queue was let
/// go. ``ViewerController`` owns one; its state is written here alone.
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
  /// 2: thumbnails read at once. Each is a JPEG the service makes from a PNG of several megabytes, so two keep the
  /// grid filling steadily without crowding out the status and picture readings, which share the service's few
  /// request slots.
  static let thumbnailsAtOnce = 2

  /// What the Activity, Apps and Screenshots tabs hold.
  private(set) var panes = ViewerPanes()

  /// The window these tabs belong to: whether it is on screen, the tab it shows, and its tab switch. Weak, as the
  /// window owns them.
  @ObservationIgnored private weak var viewer: ViewerController?
  /// The viewer service it reads.
  @ObservationIgnored private let service: any ViewerService
  /// The clock every wait runs on; a cancelled wait ends at once.
  @ObservationIgnored private let clock: any Clock<Duration>
  /// The date and time now, which the tabs tell times and days by.
  @ObservationIgnored private let wallClock: @MainActor () -> Date
  /// Where a copied screenshot goes, and how its file is shown.
  @ObservationIgnored private let shotOutlet: ShotOutlet
  /// The tabs' reading and thumbnails under way.
  @ObservationIgnored private(set) var work = ViewerPaneWork()

  /// Creates tabs that have read nothing yet.
  ///
  /// - Parameters:
  ///   - viewer: The window they belong to.
  ///   - dependencies: What they reach outside the window.
  init(viewer: ViewerController, dependencies: ViewerDependencies) {
    self.viewer = viewer
    service = dependencies.service
    clock = dependencies.clock
    wallClock = dependencies.wallClock
    shotOutlet = dependencies.shotOutlet
  }

  /// Reads the shown tab while the window is on screen, every its own pause, and stops reading any other. The
  /// thumbnails being read are let go unless Activity or Screenshots shows, and the large picture follows the window.
  /// With the window gone, nothing shows.
  func followPane() {
    work.generation += 1
    work.reading?.cancel()
    work.reading = nil
    let shown = viewer?.shown == true
    let tab = viewer?.session.tab ?? .screen
    if !shown || (tab != .activity && tab != .screenshots) { resetThumbnails() }
    followLarge()
    guard shown, tab != .screen else { return }
    let number = work.generation
    work.reading = Task { [weak self, clock] in
      while !Task.isCancelled {
        guard self?.work.generation == number else { return }
        await self?.read(tab, number)
        guard self?.work.generation == number, !Task.isCancelled else { return }
        // A cancelled pause ends at once, and the loop's condition ends the loop.
        try? await clock.sleep(for: Self.pause(for: tab))
      }
    }
  }

  /// The pause between a tab's readings.
  ///
  /// - Parameter tab: The tab.
  /// - Returns: Its pause.
  static func pause(for tab: ViewerTab) -> Duration {
    switch tab {
    case .activity: activityEvery
    case .apps: appsEvery
    case .screen, .screenshots: shotsEvery
    }
  }

  /// One reading of a tab.
  ///
  /// - Parameters:
  ///   - tab: The tab.
  ///   - number: The reading's generation; a newer one drops its reply.
  func read(_ tab: ViewerTab, _ number: Int) async {
    switch tab {
    case .activity: await readActivity(number)
    case .apps: await readApps(number)
    case .screenshots: await readShots(number)
    case .screen: return
    }
  }

  /// Applies one of the other tabs' events.
  ///
  /// - Parameter event: What the person did.
  func sendPane(_ event: ViewerEvent) {
    switch event {
    case .searchActivity(let query): panes.activity.query = query
    case .filterActivity(let filter): panes.activity.filter = filter
    case .searchApps(let query): panes.apps.query = query
    case .showWindow(let app, let window): showWindow(app: app, window: window)
    case .clearOutline: panes.apps.shown = nil
    case .searchShots(let query): panes.shots.query = query
    case .selectShot(let name): panes.shots.selected = name
    case .openShot(let name): openShot(name)
    case .toggleShot: toggleShot()
    case .previousShot: stepShot(by: -1)
    case .nextShot: stepShot(by: 1)
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
      panes.activity.failure = nil
      if let list { keep(list) }
      // What the tab now shows is seen, also while the picture, and with it the status, is paused.
      viewer?.noteActivitySeen()
    } catch {
      guard number == work.generation, !error.isCancelled else { return }
      panes.activity.failure = error.message
    }
  }

  /// Reads the apps and decodes the icons not known yet. A reply overtaken by a newer start or stop has no effect; a
  /// failure is kept to say only while nothing was read, and a cancelled request says nothing.
  ///
  /// - Parameter number: The reading's generation.
  func readApps(_ number: Int) async {
    do {
      let apps = try await service.apps()
      let icons = await Self.newIcons(apps, known: Set(panes.apps.icons.keys))
      guard number == work.generation else { return }
      panes.apps.apps = apps
      panes.apps.icons.merge(icons) { _, new in new }
      panes.apps.failure = nil
    } catch {
      guard number == work.generation, !error.isCancelled else { return }
      panes.apps.failure = error.message
    }
  }

  /// The icons of `apps` not yet known, decoded off the main actor.
  ///
  /// - Parameters:
  ///   - apps: The apps read.
  ///   - known: The icons decoded already, by what they are known by.
  /// - Returns: The new icons, by what they are known by.
  static func newIcons(_ apps: [HelperApp], known: Set<String>) async -> [String: IconImage] {
    var icons: [String: IconImage] = [:]
    for app in apps where !known.contains(app.iconID) && icons[app.iconID] == nil {
      guard let png = app.icon, let image = await ImageDecoding.image(png) else { continue }
      icons[app.iconID] = IconImage(id: app.iconID, image: image)
    }
    return icons
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
    panes.apps.shown = ViewerShownWindow(app: app.name, icon: app.icon == nil ? nil : app.iconID, window: window)
    viewer?.selectTab(.screen)
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
    panes.shots.shots = list.shots
    panes.shots.folder = list.folder
  }

  /// Shows a screenshot large on the Screenshots tab, from its grid or an Activity line's Open; a search that hides
  /// it gives way, so Previous and Next go from it.
  ///
  /// - Parameter name: The screenshot's file name.
  func openShot(_ name: String) {
    guard let viewer else { return }
    if viewer.session.tab != .screenshots { viewer.selectTab(.screenshots) }
    panes.shots.selected = name
    if !ViewerPresenter.shotOrder(viewer.snapshot).contains(name) { panes.shots.query = "" }
    showLarge(name)
  }

  /// Space: closes the large view, or shows the selected screenshot large.
  func toggleShot() {
    if panes.shots.large != nil { return closeShot() }
    guard let viewer, let name = ViewerPresenter.selectedShot(viewer.snapshot) else { return }
    showLarge(name)
  }

  /// Previous or Next, ← or →: the screenshot `offset` places along in the grid's order, never past either end; the
  /// large one while it shows, else the selection.
  ///
  /// - Parameter offset: −1 for Previous, 1 for Next.
  func stepShot(by offset: Int) {
    guard let viewer else { return }
    let order = ViewerPresenter.shotOrder(viewer.snapshot)
    guard let from = panes.shots.large ?? ViewerPresenter.selectedShot(viewer.snapshot),
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
    work.large.copyGeneration += 1
    work.large.copy?.cancel()
    work.large.copy = nil
    if let png = panes.shots.full?.png { return shotOutlet.copy(png) }
    guard let name = panes.shots.large else { return }
    let number = work.large.copyGeneration
    work.large.copy = Task { [weak self, service] in
      // A screenshot that cannot be read copies nothing; the person can copy again.
      let png = try? await service.screenshot(name: name, full: true)
      guard let self, let png, number == work.large.copyGeneration else { return }
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
  private func followLarge() {
    if viewer?.shown != true || viewer?.session.tab != .screenshots {
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

  /// Starts the waiting thumbnails, oldest first, while fewer than ``thumbnailsAtOnce`` are being read.
  private func readThumbnails() {
    while work.thumbnails.loading.count < Self.thumbnailsAtOnce, !work.thumbnails.wanted.isEmpty {
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

  /// Lets the queue go: what waits is forgotten, what is being read is cancelled, and any reply still to come is
  /// dropped. What is kept stays.
  func resetThumbnails() {
    work.thumbnails.generation += 1
    work.thumbnails.wanted.removeAll()
    for load in work.thumbnails.loading.values { load.cancel() }
    work.thumbnails.loading.removeAll()
  }
}
