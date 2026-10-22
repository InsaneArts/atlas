import AtlasKit
import AtlasUI

/// The Activity, Apps and Screenshots tabs: each reads only while it shows and the window is on screen, one reading
/// at a time on the injected pause, and a reply that arrives after its tab was left or the window hidden has no
/// effect. Their events are answered here and in `+Activity`, `+Apps`, `+Shots` and `+Thumbnails`.
extension ViewerController {
  /// 2 s: Activity's pause between readings. An agent acts every second or two, so a new line shows about as soon as
  /// its caption does on the screen; the log is a file the service reads, which asks nothing of the helper.
  static let activityEvery: Duration = .seconds(2)
  /// 3 s: Apps' pause. Listing the apps asks the helper itself for its windows, more than a file read; apps open and
  /// windows move at the person's pace, and 3 s still shows a new window before they look for it.
  static let appsEvery: Duration = .seconds(3)
  /// 5 s: the Screenshots list's pause. An agent takes one every few seconds at most while it checks its work, and
  /// the list is a folder the service reads; 5 s shows a new one within a glance.
  static let shotsEvery: Duration = .seconds(5)

  /// Reads the shown tab while the window is on screen, every its own pause, and stops reading any other. The
  /// thumbnails being read are let go unless Activity or Screenshots shows, and the large picture follows the window.
  func followPane() {
    paneWork.generation += 1
    paneWork.reading?.cancel()
    paneWork.reading = nil
    if !shown || (session.tab != .activity && session.tab != .screenshots) { resetThumbnails() }
    followLarge()
    guard shown, session.tab != .screen else { return }
    let tab = session.tab
    let number = paneWork.generation
    let nap = nap
    paneWork.reading = Task { [weak self] in
      while !Task.isCancelled {
        guard self?.paneWork.generation == number else { return }
        await self?.read(tab, number)
        guard self?.paneWork.generation == number, !Task.isCancelled else { return }
        await nap(Self.pause(for: tab))
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
}
