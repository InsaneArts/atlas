import AtlasCommands
import AtlasService
import Foundation

/// What the menu bar panel's controller reaches outside itself, handed in so tests can stand in for each: the viewer
/// service, the `atlas` command line, the helper's own commands, the login item, its links to the rest of the app,
/// the clocks its readings wait on and its words count to, and where Recent's apps' icons are found.
struct PanelDependencies {
  /// The viewer service it reads and asks.
  var service: any ViewerService
  /// The `atlas` command line its actions run.
  var commands: any CommandRunning
  /// The helper's own commands, shared with every window.
  var actions: HelperActions
  /// The app's login item.
  var loginItem: OpenAtLogin
  /// What the panel asks of the rest of the app.
  var links = PanelLinks()
  /// The clock the pauses between readings run on; a cancelled pause ends at once.
  var clock: any Clock<Duration>
  /// The time now, which a session's age counts to.
  var wallClock: @MainActor () -> Date = { Date() }
  /// Where Recent's apps' icons are found; none by default, so tests and renders never touch the disk.
  var icons = AppIcons.none
}
