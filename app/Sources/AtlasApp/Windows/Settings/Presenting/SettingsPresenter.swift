import AtlasUI

/// Decides what the Settings window shows from what its controller holds and reads: a pure function of the snapshot,
/// in `SettingsPresenter+General`, `+Helper`, `+Agents`, `+About` and `+Alert`.
enum SettingsPresenter {
  /// The window as it should look.
  ///
  /// - Parameter snapshot: What the controller holds and reads.
  /// - Returns: The window's state.
  static func state(_ snapshot: SettingsSnapshot) -> SettingsState {
    SettingsState(
      tabs: tabs(selected: snapshot.session.tab),
      general: general(snapshot),
      helper: helper(snapshot),
      agents: agents(snapshot),
      about: about(snapshot),
      alert: alert(snapshot)
    )
  }

  /// The tab row: General, Helper, Agents and About, each its symbol over its word, `selected` shown (v3 G1).
  ///
  /// - Parameter selected: The pane shown.
  /// - Returns: The tab row.
  static func tabs(selected: SettingsTab) -> CapsuleTabsState<SettingsTab> {
    CapsuleTabsState(
      tabs: SettingsTab.allCases.map { CapsuleTab(id: $0, title: SettingsWords.tab($0), symbol: symbol($0)) },
      selected: selected,
      label: SettingsWords.tabsLabel
    )
  }

  /// A tab's symbol: a gear, a display, sparkles for the agents, and an info circle for About, where the orbit no
  /// longer stands (v3 G1).
  ///
  /// - Parameter tab: The tab.
  /// - Returns: Its symbol.
  static func symbol(_ tab: SettingsTab) -> SymbolName {
    switch tab {
    case .general: .gear
    case .helper: .display
    case .agents: .sparkles
    case .about: .info
    }
  }
}
