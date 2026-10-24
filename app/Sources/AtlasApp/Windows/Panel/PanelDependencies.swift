import AtlasCommands
import AtlasService

/// What the menu bar panel's controller reaches outside itself, handed in so tests can stand in for each: the viewer
/// service, the `atlas` command line, the helper's own commands, the login item, its links to the rest of the app
/// and the clock its readings wait on.
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
}
