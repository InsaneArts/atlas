import AtlasKit
import AtlasUI
import Observation

/// The Settings window's one observed object: the pane shown, the alert asked, the agents and projects, and the
/// person's choices. Values another window also shows (the helper's status, Open at Login, the picture) are read
/// through ``SettingsLinks`` each time the window draws, so they stay one value; ``SettingsPresenter`` decides what
/// the window shows. Its parts are in `SettingsController+Helper`, `+Agents`, `+Updates` and `+Alert`.
@MainActor
@Observable
final class SettingsController {
  /// What the window is doing. Written only by the controller's own files.
  var session = SettingsSession()
  /// The agents as `atlas agents status` last reported them. Written only by the controller's own files.
  var agents = SettingsAgents()
  /// The icons of the agents whose apps are on this Mac, by agent id: read once.
  private(set) var icons: [String: IconImage] = [:]
  /// Whether Notify me is on.
  private(set) var notify = false
  /// Whether the Dock badge is on.
  private(set) var badge = false
  /// Counts the times the window came forward and a choice was kept where no other window shows it, so what the
  /// links read is read again then.
  private(set) var revision = 0

  /// What it reaches outside itself.
  @ObservationIgnored let dependencies: SettingsDependencies
  /// Reads the agents; the newest reading replaces any still under way.
  @ObservationIgnored var agentsReading: Task<Void, Never>?
  /// Numbers each agents reading, so only the newest one started is kept.
  @ObservationIgnored var agentsGeneration = 0

  /// Creates the window's controller on the pane last shown, with the person's saved choices.
  ///
  /// - Parameter dependencies: What it reaches outside itself.
  init(dependencies: SettingsDependencies) {
    self.dependencies = dependencies
    let preferences = dependencies.preferences
    session.tab =
      preferences.loadTab()
      .flatMap(SettingsTab.init(rawValue:)) ?? .general
    notify = preferences.loadNotify()
    badge = preferences.loadBadge()
    icons = dependencies.agentIcons()
  }

  /// The helper's own commands, shared with the panel and the main window.
  var actions: HelperActions { dependencies.actions }

  /// Everything the controller holds and reads now, as one value.
  var snapshot: SettingsSnapshot {
    // Read so the window draws again when it comes forward or keeps a choice no other window shows.
    _ = revision
    let links = dependencies.links
    let helper = links.status()
    return SettingsSnapshot(
      session: session,
      agents: agents,
      icons: icons,
      notify: notify,
      badge: badge,
      facts: SettingsFacts(
        status: helper.status,
        connecting: helper.connecting,
        opensAtLogin: links.opensAtLogin(),
        picture: links.picture(),
        canPause: links.canPause(),
        running: actions.running,
        installing: dependencies.installs.installing,
        version: dependencies.version
      )
    )
  }

  /// The window as it should look now.
  var state: SettingsState { SettingsPresenter.state(snapshot) }

  /// Applies one of the window's events.
  ///
  /// - Parameter event: What the person did.
  func send(_ event: SettingsEvent) {
    switch event {
    case .selectTab(let tab): selectTab(tab)
    case .checkAgain: Task { await checkAgain() }
    case .askSwitch: ask(.switchToHelper)
    case .askRecover: ask(.recover)
    case .askLogOut: ask(.logOut)
    case .connect(let id): Task { await connect(id) }
    case .askForget(let path): askForget(path)
    case .checkForUpdates: Task { await checkForUpdates() }
    case .askInstall: askInstall()
    case .gettingStarted: dependencies.links.gettingStarted()
    case .alertConfirm: Task { await confirmAlert() }
    case .alertCancel: cancelAlert()
    }
  }

  /// Writes what changed in the General pane where each value belongs: Open at Login through the panel's command,
  /// the picture to the main window or its saved rate, and Notify me and the badge to the saved choices.
  ///
  /// - Parameter general: The pane as the person left it.
  func update(_ general: SettingsGeneralState) {
    let shown = SettingsPresenter.general(snapshot)
    let links = dependencies.links
    if general.opensAtLogin != shown.opensAtLogin {
      Task { await links.setOpenAtLogin(general.opensAtLogin) }
    }
    if general.picture != shown.picture {
      links.choosePicture(SettingsPresenter.choice(general.picture))
      revision += 1
    }
    if general.notify != notify {
      notify = general.notify
      dependencies.preferences.saveNotify(notify)
    }
    if general.badge != badge {
      badge = general.badge
      dependencies.preferences.saveBadge(badge)
    }
  }

  /// Shows a pane, remembered for the next time the window opens. The Agents pane reads the agents afresh.
  ///
  /// - Parameter tab: The pane.
  func selectTab(_ tab: SettingsTab) {
    guard tab != session.tab else { return }
    session.tab = tab
    dependencies.preferences.saveTab(tab.rawValue)
    if tab == .agents { readAgents() }
  }

  /// The window came forward: what the links read is read again, and the agents and projects afresh.
  func cameForward() {
    revision += 1
    readAgents()
  }

  /// The window closed: the agents reading under way stops. A command it started, such as a check for updates or a
  /// Connect, runs on, and the app keeps this controller, so a window opened meanwhile waits for it rather than
  /// starting another.
  func close() {
    agentsReading?.cancel()
    agentsReading = nil
  }

  /// One line about what a command just did, in the pane that ran it; it stays until the next.
  ///
  /// - Parameters:
  ///   - text: The line, or nil to show none.
  ///   - tab: The pane it shows in.
  ///   - problem: Whether it says something went wrong or could not run, which draws it in the attention colour.
  func say(_ text: String?, in tab: SettingsTab, problem: Bool = false) {
    session.notice = text.map { SettingsNotice(tab: tab, text: $0, isProblem: problem) }
  }
}
