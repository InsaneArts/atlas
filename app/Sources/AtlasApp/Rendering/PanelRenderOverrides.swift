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
/// - `ATLAS_RENDER_ALERT` (`recover`, `switch`, `logOut` or `logOutAnyway`) draws the alert window instead, and
///   `ATLAS_RENDER_BUSY=1` draws it while its command runs.
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
  /// Whether a set-up Mac's facts fill in what the live service lacks.
  var showsEverything: Bool

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
    showsEverything = environment["ATLAS_RENDER_PREVIEW"] == "1"
  }

  /// The alert window's alert to draw, as its controller would hold it, over the panel's overview; nil draws the
  /// panel.
  ///
  /// - Parameter overview: The overview the panel shows, which says why Recover helps.
  /// - Returns: The alert's snapshot, running or saying why its command did not run as asked.
  func alertSnapshot(over overview: ViewerStatus.Overview) -> AlertSnapshot? {
    alert.map { AlertSnapshot(alert: $0, running: busy, failure: message.map(Words.sentence), overview: overview) }
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
      shown.opensAtLogin = OpenAtLogin.State(isAvailable: true, isEnabled: true)
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

  /// Three actions an agent took in Opie, newest first: a few seconds ago while it works, else two minutes ago, so a
  /// set-up Mac's Idle offers no Stop.
  ///
  /// - Parameter working: Whether the agent works now.
  /// - Returns: The actions.
  private static func recent(working: Bool) -> [ViewerStatus.Activity] {
    let ages: [Double] = working ? [2, 9, 31] : [120, 129, 151]
    let said = [
      working ? "turning Show Center off in Opie" : "turned Show Center off", "clicked Ring in Opie", "opened Opie",
    ]
    return zip(said, ages)
      .map { ViewerStatus.Activity(operation: "input", ok: true, ageSeconds: $1, says: $0) }
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
