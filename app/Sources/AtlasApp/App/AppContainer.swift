import AppKit
import AtlasCommands
import AtlasService

/// Everything the running app builds, each live object once, with every connection made: the helper's commands and
/// the control work they share, agent installs, the menu bar panel (`AppContainer+Panel`), the notification
/// (`AppContainer+Attention`), the saved choices, each window's controller and its dependencies
/// (`AppContainer+Parts`), and the coordinator that opens the windows. The one live clock comes from `AtlasApp`, and
/// this is the one place the app's standard defaults are reached.
final class AppContainer {
  /// The `atlas` command line every window runs.
  let commands: any CommandRunning
  /// The viewer service the panel and viewer read.
  let service: any ViewerService
  /// The app's login item, for the panel and the setup window.
  let loginItem: OpenAtLogin
  /// The clock every window's waits run on: the one `AtlasApp` makes.
  let clock: any Clock<Duration>
  /// Connecting agents, which the setup window and Settings share, so one install runs at a time and outlives the
  /// window that started it.
  let installs: AgentInstalls
  /// The main window's choices that outlast it, in the standard defaults; Settings saves the rate in them while no
  /// main window is open.
  let viewerPreferences: ViewerPreferences
  /// Settings' choices that outlast it, in the standard defaults; the signals read Notify me from them.
  let settingsPreferences: SettingsPreferences
  /// Opens and closes every window.
  let coordinator: WindowCoordinator
  /// The app's one icon repository and disk store, shared by the panel and every main window, including reopened
  /// windows. It owns the concurrency limit, coalesced requests and bounded decoded cache.
  let appIcons: AppIcons
  /// The helper's own commands, which the panel and the main window share, so one of each runs at a time; made at
  /// launch.
  private(set) var actions: HelperActions?
  /// The panel's controller, which the viewer opens on; made at launch.
  var panel: PanelController?
  /// The menu bar item's panel; made at launch.
  var menuBar: PanelPopover?
  /// The notification for when Atlas needs the person; made at launch.
  var signals: AttentionSignals?

  /// An app that runs `commands` and reads `service`. Nothing runs until ``start()``.
  ///
  /// - Parameters:
  ///   - commands: The `atlas` command line.
  ///   - service: The viewer service.
  ///   - loginItem: The app's login item.
  ///   - clock: The clock every window's waits run on.
  init(commands: any CommandRunning, service: any ViewerService, loginItem: OpenAtLogin, clock: any Clock<Duration>) {
    self.commands = commands
    self.service = service
    self.loginItem = loginItem
    self.clock = clock
    installs = AgentInstalls(commands: commands)
    let viewerPreferences = ViewerPreferences.stored(in: UserDefaults.standard)
    self.viewerPreferences = viewerPreferences
    settingsPreferences = .stored(in: UserDefaults.standard)
    appIcons = .live(home: FileManager.default.homeDirectoryForCurrentUser)
    coordinator = WindowCoordinator(windows: .live, viewerPreferences: viewerPreferences)
    coordinator.parts = self
  }

  /// Builds the app's control work, the signals and the menu bar panel, once the app has launched.
  func start() {
    // The app's control work outlives any one main window: a release a closed window sent is still awaited by the
    // next command, and Take Control in a reopened window waits for it. Before a switch, a log-out or a restart, the
    // main window's control ends and its release joins the barrier.
    let barrier = ControlBarrier(service: service, clock: clock)
    let actions = HelperActions(commands: commands, service: service, barrier: barrier) { [weak coordinator] in
      coordinator?.releaseViewerControl()
    }
    self.actions = actions
    startSignals()
    startPanel(actions: actions)
  }
}
