import AtlasStatus
import AtlasUI

extension PanelPresenter {
  /// The row of buttons, the actions on the agent: Stop while an agent works, or Resume once agents are stopped, then
  /// the state's one main action. The pause button sits on the picture, and ⋯ in the header.
  ///
  /// - Parameters:
  ///   - snapshot: What the controller holds.
  ///   - mode: The helper's mode.
  /// - Returns: The buttons.
  static func controls(_ snapshot: PanelSnapshot, mode: Mode) -> PanelControls {
    let agent = agentButton(snapshot, mode: mode)
    return PanelControls(agent: agent, agentHelp: agentHelp(agent), main: main(snapshot, mode: mode))
  }

  /// Stop's or Resume's tooltip, as the main window's button says it (``ShortcutWords``).
  ///
  /// - Parameter agent: The button, or nil for none.
  /// - Returns: The tooltip; empty for no button.
  private static func agentHelp(_ agent: ViewerAgentButton?) -> String {
    switch agent {
    case .stop: ShortcutWords.stopAgent
    case .resume: ShortcutWords.resumeAgent
    case nil: ""
    }
  }

  /// The one main action for the state, never shown greyed: Give Back in control; Recover… while the helper needs
  /// the person or isn't ready and recovering would help, or else Open Settings, which shows what the helper needs;
  /// Switch to the Helper… while logged out, waiting only while a switch runs; and otherwise Open Atlas, which works in
  /// every state, connecting and stopped included, as the main window has a stage of its own for each. Recover… opens
  /// its alert even while a recovery runs from another window, and the alert shows it running.
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
      guard snapshot.status.overview.recoveryHelps else {
        return PanelMainButton(title: PanelWords.openSettings, help: PanelWords.openSettingsHelp, event: .openSettings)
      }
      return PanelMainButton(title: HelperWords.recoverItem, help: PanelWords.recoverHelp, event: .askRecover)
    case .loggedOut:
      return PanelMainButton(
        title: HelperWords.switchItem,
        isEnabled: !snapshot.running.contains(.switchToHelper),
        help: HelperWords.switchHelp,
        event: .askSwitch
      )
    case .idle, .working, .stopped, .connecting, .inUseElsewhere:
      return PanelMainButton(title: PanelWords.openAtlas, help: PanelWords.openAtlasHelp, event: .openAtlas)
    }
  }

  /// The ⋯ menu: Open Atlas leads it only while the main button is something else, so the panel never offers it
  /// twice.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The menu.
  static func moreMenu(_ snapshot: PanelSnapshot) -> PanelMoreMenuState {
    PanelMoreMenuState(showsOpenAtlas: main(snapshot, mode: mode(snapshot)).event != .openAtlas)
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
