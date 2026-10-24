import AtlasStatus
import AtlasUI

extension PanelPresenter {
  /// The row of buttons: Stop while an agent works, the state's one main action, the pause button and ⋯.
  ///
  /// - Parameters:
  ///   - snapshot: What the controller holds.
  ///   - mode: The helper's mode.
  /// - Returns: The buttons.
  static func controls(_ snapshot: PanelSnapshot, mode: Mode) -> PanelControls {
    let paused = snapshot.session.paused
    return PanelControls(
      showsStop: agentButton(snapshot, mode: mode) == .stop,
      main: main(snapshot, mode: mode),
      pause: PanelPause(
        isPaused: paused,
        label: paused ? PanelWords.resumePicture : PanelWords.pausePicture,
        help: paused ? PanelWords.resumePictureHelp : PanelWords.pausePictureHelp
      ),
      isMoreOpen: snapshot.session.moreOpen
    )
  }

  /// The one main action for the state (docs/atlas-app-build-plan.md, "Main action by state"): Give Back in
  /// control, Recover… while the helper needs the person or isn't ready, Resume once stopped, Switch to the Helper…
  /// while logged out, and otherwise Open Atlas, which waits while connecting. Each waits while its own command runs.
  ///
  /// - Parameters:
  ///   - snapshot: What the controller holds.
  ///   - mode: The helper's mode.
  /// - Returns: The main action.
  static func main(_ snapshot: PanelSnapshot, mode: Mode) -> PanelMainButton {
    switch mode.kind {
    case .control:
      return PanelMainButton(title: PanelWords.giveBack, help: PanelWords.giveBackHelp, event: .giveBack)
    case .needsYou, .notReady:
      return PanelMainButton(
        title: HelperWords.recoverItem,
        isEnabled: helperMenu(snapshot, mode: mode).canRecover,
        help: PanelWords.recoverHelp,
        event: .askRecover
      )
    case .stopped:
      return PanelMainButton(
        title: PanelWords.resume,
        isEnabled: agentButton(snapshot, mode: mode) == .resume(isEnabled: true),
        help: PanelWords.resumeHelp,
        event: .resumeAgent
      )
    case .loggedOut:
      return PanelMainButton(
        title: HelperWords.switchItem,
        isEnabled: helperMenu(snapshot, mode: mode).canSwitch,
        help: HelperWords.switchHelp,
        event: .askSwitch
      )
    case .idle, .working, .connecting, .inUseElsewhere:
      return PanelMainButton(
        title: PanelWords.openAtlas,
        isEnabled: mode.kind != .connecting,
        help: PanelWords.openAtlasHelp,
        event: .openAtlas
      )
    }
  }

  /// The ⋯ menu: which of the helper's commands it offers, and Open at Login's tick, shown only when the app carries
  /// the login item.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The menu.
  static func moreMenu(_ snapshot: PanelSnapshot) -> PanelMoreMenuState {
    PanelMoreMenuState(
      helper: helperMenu(snapshot, mode: mode(snapshot)),
      opensAtLogin: snapshot.opensAtLogin.flatMap { $0.isAvailable ? $0.isEnabled : nil }
    )
  }

  /// Which of the helper's commands the panel offers, its main action and its ⋯ menu alike: Recover… only where it
  /// would help, while the helper needs the person or isn't ready; Switch to the Helper… always; Log Out the Helper…
  /// where the helper's mode allows it. Each waits while its command runs from any window.
  ///
  /// - Parameters:
  ///   - snapshot: What the controller holds.
  ///   - mode: The helper's mode.
  /// - Returns: The commands offered.
  static func helperMenu(_ snapshot: PanelSnapshot, mode: Mode) -> HelperMenuState {
    let running = snapshot.running
    // The main window's rule: Recover… only where it fixes something, an unknown outcome or input Recover starts.
    let helps = snapshot.status.overview.recoveryHelps
    return HelperMenuState(
      canRecover: helps && !running.contains(.recovery),
      canRestart: false,
      canSwitch: !running.contains(.switchToHelper),
      canLogOut: snapshot.status.overview.mode.allowsLogOut && !running.contains(.logOut)
    )
  }

  /// Stop or Resume, by the rule the main window's top row follows too; Resume waits while the panel's own request
  /// runs.
  ///
  /// - Parameters:
  ///   - snapshot: What the controller holds.
  ///   - mode: The helper's mode.
  /// - Returns: The button, or nil for none.
  private static func agentButton(_ snapshot: PanelSnapshot, mode: Mode) -> ViewerAgentButton? {
    mode.agentButton(
      snapshot.status.overview,
      holdingControl: snapshot.holdingControl,
      isBusy: snapshot.session.requests > 0 || snapshot.agentRequests > 0
    )
  }
}
