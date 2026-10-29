import AppKit
import AtlasCommands
import AtlasService

/// What the main window's controller reaches outside itself, handed in so tests can stand in for each: the viewer
/// service, the helper's own commands, the clock its waits run on, the time now, the double-click speed, where pictures
/// and screenshots go, where apps' icons are found, the saved choices and switching to the helper.
struct ViewerDependencies {
  /// The viewer service it reads and asks.
  var service: any ViewerService
  /// The helper's own commands, shared with the panel.
  var actions: HelperActions
  /// Where the rate, the corner and the sidebar are kept between windows.
  var preferences: ViewerPreferences
  /// The time now, in seconds of uptime, which never steps back: what a picture's age is told by.
  var uptime: @MainActor () -> TimeInterval
  /// The clock every wait runs on: the pause between readings and the tabs', the heartbeat, the picture's tick, gathering
  /// scrolls, the reading input asks for, and Take Control's retry. A cancelled wait ends at once.
  var clock: any Clock<Duration>
  /// One heartbeat tick; the app passes ``ViewerControl/heartbeatEvery``.
  var heartbeatInterval: Duration
  /// The Mac's double-click speed, in seconds, read at each click so a change in System Settings counts at once.
  var doubleClickInterval: @MainActor () -> TimeInterval
  /// Where a copied or saved picture goes.
  var pictures: PictureOutlet
  /// Locks the screen so the person can log in to the helper, returning what to tell them; nil when the app cannot.
  var switchToHelper: (@MainActor () async -> String)?
  /// The agents that already know how to use Atlas, read for the helper's facts; nil when they could not be read.
  var connectedAgents: @MainActor () async -> [AgentID]? = { nil }
  /// When an account logged in, if it is logged in now: the helper's Logged in fact.
  var loginTime: @MainActor (String) -> Date? = { _ in nil }
  /// The date and time now, which the tabs tell times and days by.
  var wallClock: @MainActor () -> Date = { Date() }
  /// Where a copied screenshot goes, and how its file is shown; by default nowhere.
  var shotOutlet = ShotOutlet(copy: { _ in }, reveal: { _ in })
  /// Opens Settings on Helper, which says what the helper needs.
  var openSettings: @MainActor () -> Void = {}
  /// Where apps' icons are found and kept; by default none is, and every app draws its tile.
  var icons = AppIcons.none
}
