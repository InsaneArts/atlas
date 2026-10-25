import AtlasUI
import Foundation
import Observation

/// The Settings window's one observed object: the pane shown, the alert asked, the agents and projects, and the
/// person's choices. Values another window also shows (the helper's status, Open at Login, the picture) are read
/// through ``SettingsLinks`` each time the window draws, so they stay one value; ``SettingsPresenter`` decides what
/// the window shows. Its parts are in `SettingsController+Helper`, `+Agents`, `+Updates`, `+Alert` and `+Window`.
@Observable
final class SettingsController {
  /// What the window is doing. Written only by the controller's own files.
  var session = SettingsSession()
  /// The agents as `atlas agents status` last reported them. Written only by the controller's own files.
  var agents = SettingsAgents()
  /// The icons of the agents whose apps are on this Mac, by agent id: read once, through the dependencies.
  private(set) var icons: [String: IconImage] = [:]
  /// Whether Notify me is on.
  private(set) var notify = false
  /// Counts the times the window came forward and a choice was kept where no other window shows it, so what the
  /// links read is read again then.
  private(set) var revision = 0

  /// What it reaches outside itself.
  @ObservationIgnored let dependencies: SettingsDependencies
  /// The moment a Recover that worked holds before its alert closes, which Done ends at once.
  @ObservationIgnored let doneHold = DoneHold()
  /// Reads the agents; the newest reading replaces any still under way.
  @ObservationIgnored var agentsReading: Task<Void, Never>?
  /// Numbers each agents reading, so only the newest one started is kept.
  @ObservationIgnored var agentsGeneration = 0
  /// Updates the native window title without changing its frame.
  @ObservationIgnored var nameWindow: (@MainActor (String) -> Void)?
  /// Normalized local metadata, reused until the loaded presentation changes.
  @ObservationIgnored var searchIndex: SettingsSearchIndex?

  /// Creates the window's controller on the pane last shown, with the person's saved choices.
  ///
  /// - Parameter dependencies: What it reaches outside itself.
  init(dependencies: SettingsDependencies) {
    self.dependencies = dependencies
    let preferences = dependencies.preferences
    let root = SettingsRoot(savedValue: preferences.loadTab())
    session.navigation = SettingsNavigation(initial: .root(root))
    notify = preferences.loadNotify()
    icons = AgentRowIcons.onThisMac(home: FileManager.default.homeDirectoryForCurrentUser) { path in
      dependencies.icon(forFile: path)
    }
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
      facts: SettingsFacts(
        status: helper.status,
        connecting: helper.connecting,
        opensAtLogin: links.opensAtLogin(),
        picture: links.picture(),
        canPause: links.canPause(),
        running: actions.running,
        installing: dependencies.installs.installing,
        isRemoving: dependencies.installs.isRemoving,
        version: dependencies.version
      )
    )
  }

  /// The window as it should look now.
  var state: SettingsState {
    var state = SettingsPresenter.state(snapshot)
    state.search = searchState(for: state)
    state.reveal = session.reveal
    return state
  }

  /// Applies one of the window's events.
  ///
  /// - Parameter event: What the person did.
  func send(_ event: SettingsEvent) {
    // Unstructured, as `send` is the view's synchronous callback: each async event runs in a task of its own.
    switch event {
    case .focusPage:
      session.reveal = SettingsReveal(
        target: .page(state.location),
        request: (session.reveal?.request ?? 0) + 1,
        scrolls: false
      )
    case .selectTab(let tab): selectTab(tab)
    case .navigate(let destination): navigate(destination)
    case .search(let query): search(query)
    case .find:
      if session.alert == nil { session.search.focusRequests += 1 }
    case .moveSearchSelection(let step): moveSearchSelection(step)
    case .selectSearchResult(let target):
      if session.alert == nil, state.search.results.contains(where: { $0.id == target }) {
        session.search.selection = target
      }
    case .openSearchResult(let target): openSearchResult(target)
    case .openSelectedSearchResult: openSearchResult(state.search.selection)
    case .setOpenAtLogin(let on):
      var general = SettingsPresenter.general(snapshot)
      general.opensAtLogin = on
      update(general)
    case .choosePicture(let picture):
      var general = SettingsPresenter.general(snapshot)
      general.picture = picture
      update(general)
    case .setNotify(let on):
      var general = SettingsPresenter.general(snapshot)
      general.notify = on
      update(general)
    case .copyDiagnostics: dependencies.links.copyDiagnostics()
    case .back: moveInHistory(forward: false)
    case .forward: moveInHistory(forward: true)
    case .checkAgain: Task { await checkAgain() }
    case .askSwitch: ask(.switchToHelper)
    case .askRecover: ask(.recover)
    case .askLogOut: ask(.logOut)
    case .closeLeftovers: Task { await closeLogin(.closeLeftovers) }
    case .closeHelper: Task { await closeLogin(.closeHelper) }
    case .connect(let id): Task { await connect(id) }
    case .disconnect(let id): Task { await disconnect(id) }
    case .askForget(let path): askForget(path)
    case .checkForUpdates: Task { await checkForUpdates() }
    case .askInstall: askInstall()
    case .gettingStarted: dependencies.links.gettingStarted()
    case .showAppInFinder: dependencies.revealFile(Bundle.main.bundlePath)
    case .showAgentFile(let id): showAgentFile(id)
    case .alertConfirm: Task { await confirmAlert() }
    case .alertCancel: cancelAlert()
    }
  }

  /// Writes what changed in the General pane where each value belongs: Open at Login through the panel's command,
  /// the picture to the main window or its saved rate, and Notify me to the saved choices.
  ///
  /// - Parameter general: The pane as the person left it.
  func update(_ general: SettingsGeneralState) {
    let shown = SettingsPresenter.general(snapshot)
    let links = dependencies.links
    if general.opensAtLogin != shown.opensAtLogin {
      // Unstructured, as an edit is synchronous: the change runs beside it.
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
  }

  /// Shows a pane, remembered for the next time the window opens. The Agents pane reads the agents afresh.
  ///
  /// - Parameter tab: The pane.
  func selectTab(_ tab: SettingsTab) {
    navigate(.root(SettingsRoot(pane: tab)))
  }

  /// Visits a page while retaining the shared work, notices and alert.
  ///
  /// - Parameter destination: The requested root or detail.
  func navigate(_ destination: SettingsDestination) {
    guard session.alert == nil else { return }
    session.search.query = ""
    session.search.selection = nil
    guard destination != session.navigation.destination else { return }
    session.navigation.visit(destination)
    session.reveal = SettingsReveal(target: .page(destination), request: (session.reveal?.request ?? 0) + 1)
    enteredDestination()
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
    session.search = SettingsSearchState()
    session.announcement = nil
    session.reveal = nil
    let root = session.navigation.destination?.root ?? .general
    session.navigation = SettingsNavigation(initial: .root(root))
  }

  /// One line about what a command just did, in the pane that ran it; it stays until the next.
  ///
  /// - Parameters:
  ///   - text: The line, or nil to show none.
  ///   - tab: The pane it shows in.
  ///   - kind: How it reads: quiet by default, a spinner while what it waits for runs, or the problem line.
  ///   - fromCheck: Whether Check Again said it, which places it under "Ready to act"; not by default.
  func say(_ text: String?, in tab: SettingsTab, kind: SettingsNote.Kind = .quiet, fromCheck: Bool = false) {
    session.notice = text.map { SettingsNotice(tab: tab, text: $0, kind: kind, fromCheck: fromCheck) }
  }
}
