import AtlasCommands
import AtlasService
import AtlasStatus
import AtlasUI
import Foundation
import Observation

/// The menu bar panel's one observed object: it holds the latest status, the helper's screen and what the panel is
/// doing, reads them on a schedule (`PanelController+Reading`), and runs what the panel's events ask
/// (`PanelController+Actions`). Recover… and Switch… ask first in an alert that ``WindowCoordinator`` places;
/// ``PanelPresenter`` decides what the panel shows.
///
/// Each part of the snapshot is its own stored property, so the menu bar glyph, which reads only the status, is not
/// redrawn when a picture arrives. They are written only by the controller's own files; the shell calls its methods.
@Observable
final class PanelController {
  /// Between status readings with the panel open, and between pictures while it is open: 2 s, as the viewer's
  /// default rate, so the open panel looks live.
  static let openEvery: Duration = .seconds(2)
  /// Between status readings with the panel closed: 5 s, often enough for the menu bar glyph to follow the helper's
  /// mode, rarely enough to cost nothing.
  static let closedEvery: Duration = .seconds(5)
  /// How long a state that asks something of the person holds before VoiceOver hears it: 1 s, as in the main window,
  /// so a state that passes at once is never said.
  static let announceAfter: Duration = .seconds(1)

  /// The latest status the service reported: every field at its safe default, as a service that cannot be reached
  /// reads, before the first reading ends and after one that fails, which the panel shows alike.
  var status = ViewerStatus()
  /// How the last status reading ended: nil before the first one, false when it failed, true when the service
  /// answered. Only an answer is handed to the viewer as a reading (``answer``).
  var answered: Bool?
  /// The helper's screen, when the last capture gave one; the viewer window opens on it.
  var picture: DesktopPicture?
  /// What the panel is doing: paused, an action of its own under way, its notice and its ⋯ menu.
  var session = PanelSession()
  /// The helper's short name as `atlas switch --list` last reported it: read at launch, each time the panel opens and
  /// when Getting Started closes. A listing that fails keeps the last one.
  var listedHelper: String?
  /// Where Open at Login stands, as last read: at launch, each time the panel opens, and after it is set here.
  var opensAtLogin: OpenAtLogin.State

  /// The viewer service it reads and asks.
  @ObservationIgnored let service: any ViewerService
  /// The `atlas` command line its actions run.
  @ObservationIgnored let commands: any CommandRunning
  /// The helper's own commands, shared with every window.
  @ObservationIgnored let actions: HelperActions
  /// The app's login item.
  @ObservationIgnored let loginItem: OpenAtLogin
  /// What the panel asks of the rest of the app.
  @ObservationIgnored let links: PanelLinks
  /// The clock the pauses between readings run on; a cancelled pause ends at once.
  @ObservationIgnored let clock: any Clock<Duration>
  /// The time now, which the snapshot carries.
  @ObservationIgnored let wallClock: @MainActor () -> Date
  /// The readings under way.
  @ObservationIgnored var polling: [Task<Void, Never>] = []
  /// Whether the panel is open, so the picture is read.
  var isOpen = false
  /// Numbers each status reading, so only the newest one started is shown.
  @ObservationIgnored var statusReading = 0
  /// Numbers each picture reading, under the same rule.
  @ObservationIgnored var frameReading = 0
  /// The newest Open at Login the person asked for and no change has taken up yet.
  @ObservationIgnored var wantedAtLogin: Bool?
  /// Whether a change to Open at Login runs now: one at a time, so the system ends where the person last asked.
  @ObservationIgnored var changingAtLogin = false
  /// What VoiceOver was last handed (``askingStatus``), so a reading that says the same again, or the panel opening
  /// on it, says nothing more.
  @ObservationIgnored var askedBefore: String?
  /// The wait before the newest asking state is said; nil while none waits.
  @ObservationIgnored var announcing: Task<Void, Never>?

  /// Creates a panel that has read nothing yet.
  ///
  /// - Parameter dependencies: What it reaches outside itself.
  init(dependencies: PanelDependencies) {
    service = dependencies.service
    commands = dependencies.commands
    actions = dependencies.actions
    loginItem = dependencies.loginItem
    links = dependencies.links
    clock = dependencies.clock
    wallClock = dependencies.wallClock
    opensAtLogin = dependencies.loginItem.state
  }

  /// Everything the controller holds, as one value, with whether the main window holds control and which of the
  /// helper's commands run.
  var snapshot: PanelSnapshot {
    PanelSnapshot(
      status: status,
      connecting: connecting,
      reconnecting: answered == false,
      picture: picture,
      session: session,
      holdingControl: links.holdingControl(),
      running: actions.running,
      agentRequests: actions.agentRequests,
      opensAtLogin: opensAtLogin,
      date: wallClock()
    )
  }

  /// Whether the panel waits for the service: before its first answer, and again after a reading that failed, until it
  /// answers once more, as the main window reconnects.
  var connecting: Bool { answered != true }

  /// The service's last answer, which the viewer opens on; nil before the first reading ends and after one that
  /// failed.
  var answer: ViewerStatus? { answered == true ? status : nil }

  /// The panel as it should look now.
  var state: PanelState { PanelPresenter.state(snapshot) }

  /// The ⋯ menu as it should read now.
  var moreMenu: PanelMoreMenuState { PanelPresenter.moreMenu(snapshot) }

  /// The helper to switch to: the account the service reports, which follows a helper set up meanwhile, else the one
  /// `atlas switch --list` last reported.
  var helper: String? { status.overview.account ?? listedHelper }

  /// The helper's mode, as the menu bar glyph shows it: read from the status and control alone, so the glyph is not
  /// redrawn when a picture arrives or a command starts.
  var mode: Mode {
    PanelPresenter.mode(
      PanelSnapshot(status: status, connecting: connecting, holdingControl: links.holdingControl())
    )
  }

  /// Applies one of the panel's events. ⋯ is opened by the panel's shell, which builds its menu.
  ///
  /// - Parameter event: What the person did.
  func send(_ event: PanelEvent) {
    // Unstructured, as `send` is the view's synchronous callback: each async event runs in a task of its own.
    switch event {
    case .openAtlas: links.openViewer(nil)
    case .openScreen: links.openViewer(.screen)
    case .openActivity: links.openSessions(.all)
    case .openSession(let key): links.openSessions(.one(key))
    case .showAllSessions: links.openSessions(nil)
    case .stopAgent: Task { await stopAgent() }
    case .resumeAgent: Task { await resumeAgent() }
    case .giveBack: giveBack()
    case .askRecover: links.showAlert(.recover)
    case .askSwitch: links.showAlert(.switchToHelper)
    case .openSettings: links.openSettings()
    case .togglePause: togglePause()
    case .showMore: break
    }
  }

  /// Marks the ⋯ menu open or closed, so its button draws pressed while it is open.
  ///
  /// - Parameter open: Whether the menu is open.
  func setMoreOpen(_ open: Bool) {
    session.moreOpen = open
  }
}
