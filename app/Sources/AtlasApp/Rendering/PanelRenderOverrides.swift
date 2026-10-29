import AtlasService
import AtlasUI

/// What `--render` changes in the live status for design checks, read from its environment, so every state of the
/// panel and its alert window can be drawn with no helper at all:
/// - `ATLAS_RENDER_MODE` sets the state: a helper mode the service names (`observing`, `agent`, `controlling`,
///   `recovering`, `stopped`, `blocked`, `unavailable`, `loggedOut`), with the state the service gives it, or
///   `elsewhere` (another window holds control) or `connecting` (no answer yet). `controlling` reads as Atlas's own
///   main window holding control.
/// - `ATLAS_RENDER_MESSAGE` shows what an action said, in place of the line, or in the alert window why its command
///   did not run.
/// - `ATLAS_RENDER_PAUSED=1` pauses the picture.
/// - `ATLAS_RENDER_ALERT` (`recover`, `switch`, `logOut` or `logOutAnyway`) draws the alert window instead,
///   `ATLAS_RENDER_BUSY=1` draws it while its command runs, and `ATLAS_RENDER_ALERT_PHASE=done` once it worked.
/// - `ATLAS_RENDER_NOTICE` (`setUp` or `runningAgain`) draws one of the helper's notices instead.
/// - `ATLAS_RENDER_SESSIONS` (`one`, `two`, `waiting`, `needsYou` or `more`) shows that many agents at work, each in
///   its own host session (``PanelRenderSessions``), in place of the live sessions and rows.
/// - `ATLAS_RENDER_PREVIEW=1` shows a set-up Mac: the helper's names and display, three recent actions, Open at Login
///   and, in a debug build, the sample screen when the live one gives none.
struct PanelRenderOverrides: Equatable, Sendable {
  /// The helper's mode to show in place of the live one; nil keeps it.
  var mode: HelperMode?
  /// Whether the panel shows as before its first answer.
  var connecting: Bool
  /// Whether Atlas's main window holds control.
  var holdingControl: Bool
  /// What an action said, shown in place of the line; nil shows none.
  var message: String?
  /// Whether the picture is paused.
  var paused: Bool
  /// The alert window's alert to draw instead of the panel; nil draws the panel.
  var alert: HelperAlert?
  /// Whether the alert's command runs.
  var busy: Bool
  /// Whether the alert's command worked, so it holds on its done title.
  var done: Bool
  /// The helper notice to draw instead of the panel; nil draws none.
  var notice: NoticeRender?
  /// Whether a set-up Mac's facts fill in what the live service lacks.
  var showsEverything: Bool
  /// The host sessions to show in place of the live ones; nil keeps them.
  var sessions: PanelRenderSessions?

  /// Reads the overrides from an environment.
  ///
  /// - Parameter environment: The process's environment variables.
  init(environment: [String: String]) {
    let named = environment["ATLAS_RENDER_MODE"] ?? ""
    mode =
      switch named {
      case "elsewhere": .controlling
      default: HelperMode(rawValue: named)
      }
    connecting = named == "connecting"
    holdingControl = named == "controlling"
    message = environment["ATLAS_RENDER_MESSAGE"].flatMap { $0.isEmpty ? nil : $0 }
    paused = environment["ATLAS_RENDER_PAUSED"] == "1"
    alert =
      switch environment["ATLAS_RENDER_ALERT"] {
      case "recover": .recover
      case "switch": .switchToHelper
      case "logOut": .logOut
      case "logOutAnyway": .logOutAnyway
      default: nil
      }
    busy = environment["ATLAS_RENDER_BUSY"] == "1"
    done = environment["ATLAS_RENDER_ALERT_PHASE"] == "done"
    notice = environment["ATLAS_RENDER_NOTICE"].flatMap(NoticeRender.init(rawValue:))
    showsEverything = environment["ATLAS_RENDER_PREVIEW"] == "1"
    sessions = environment["ATLAS_RENDER_SESSIONS"].flatMap(PanelRenderSessions.init(rawValue:))
  }

  /// The alert window's alert to draw, as its controller would hold it, over the panel's overview; nil draws the
  /// panel.
  ///
  /// - Parameter overview: The overview the panel shows, which says why Recover helps.
  /// - Returns: The alert's snapshot, running or saying why its command did not run as asked.
  func alertSnapshot(over overview: ViewerStatus.Overview) -> AlertSnapshot? {
    alert.map { alert in
      AlertSnapshot(
        alert: alert,
        running: busy,
        failure: message.map(Words.sentence),
        done: done,
        overview: overview
      )
    }
  }

  /// A snapshot with these overrides applied.
  ///
  /// - Parameter snapshot: What the live service showed.
  /// - Returns: The snapshot to draw.
  func applied(to snapshot: PanelSnapshot) -> PanelSnapshot {
    var shown = snapshot
    if showsEverything {
      shown.status.overview.account = "atlas"
      shown.status.overview.accountName = "Atlas"
      shown.status.overview.display = ViewerStatus.Size(width: 2560, height: 1440)
      shown.status.overview.activity = Self.recent(working: mode == .agent)
      #if DEBUG
        if let key = AppIconKey(bundleId: Self.opie) { shown.icons[key.text] = SampleAppIcons.icons[Self.opie] }
      #endif
      shown.opensAtLogin = OpenAtLogin.State(isAvailable: true, isEnabled: true)
    }
    if let sessions {
      shown.status.overview = sessions.applied(to: shown.status.overview)
    }
    if let mode {
      shown.status.overview.mode = mode
      shown.status.overview.state = Self.state(of: mode, recoverable: shown.status.overview.recoverable)
      shown.status.overview.label = ""
      shown.status.overview.reason = ""
    }
    if connecting {
      // Before the first answer the panel holds nothing the service said, and no picture.
      shown.status = ViewerStatus()
      shown.picture = nil
    }
    shown.connecting = connecting
    shown.holdingControl = holdingControl
    shown.session.notice = message
    shown.session.paused = paused
    return shown
  }

  /// Opie's bundle identifier, as the samples know it.
  private static let opie = "com.insanearts.opie"

  /// Three actions, newest first, as the panel's samples have them: an agent's in Opie, Atlas recovering the helper,
  /// and the person's own click, a few seconds ago while it works, else two minutes ago, so a set-up Mac's Idle
  /// offers no Stop.
  ///
  /// - Parameter working: Whether the agent works now.
  /// - Returns: The actions.
  private static func recent(working: Bool) -> [ViewerStatus.Activity] {
    let ages: [Double] = working ? [2, 9, 31] : [120, 129, 151]
    return [
      ViewerStatus.Activity(
        operation: "input",
        ok: true,
        ageSeconds: ages[0],
        says: working ? "turning Show Center off in Opie" : "turned Show Center off",
        app: "Opie",
        bundleId: Self.opie
      ),
      ViewerStatus.Activity(operation: "restart", ok: true, ageSeconds: ages[1], says: "recovered the helper"),
      ViewerStatus.Activity(
        operation: "input",
        ok: true,
        actor: "viewer",
        ageSeconds: ages[2],
        says: "clicked in Opie",
        app: "Opie",
        bundleId: Self.opie
      ),
    ]
  }

  /// The state the service gives a mode (`src/lib/overview.rs`, `kind()`), for renders only: the app itself always
  /// reads the state the service sends.
  ///
  /// - Parameters:
  ///   - mode: The mode the render shows.
  ///   - recoverable: Whether Recover fixes what keeps a not-ready helper not ready.
  /// - Returns: The state.
  private static func state(of mode: HelperMode, recoverable: Bool) -> OverviewState {
    switch mode {
    case .agent: .working
    case .observing: .idle
    case .controlling: .control
    case .recovering: .needsYou
    case .stopped: .stopped
    case .blocked where recoverable: .needsYou
    case .blocked, .unavailable, .loggedOut: .notReady
    }
  }
}
