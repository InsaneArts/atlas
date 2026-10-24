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

  /// The tab row: General, Helper, Agents and About with their icons, `selected` shown.
  ///
  /// - Parameter selected: The pane shown.
  /// - Returns: The tab row.
  static func tabs(selected: SettingsTab) -> IconTabsState<SettingsTab> {
    IconTabsState(
      tabs: SettingsTab.allCases.map { IconTab(id: $0, title: SettingsWords.tab($0), icon: icon($0)) },
      selected: selected,
      label: SettingsWords.tabsLabel
    )
  }

  /// A tab's icon: a gear, a display, two people, and Atlas's orbit for About.
  ///
  /// - Parameter tab: The tab.
  /// - Returns: Its icon.
  static func icon(_ tab: SettingsTab) -> TabIcon {
    switch tab {
    case .general: .symbol("gearshape")
    case .helper: .symbol("display")
    case .agents: .symbol("person.2")
    case .about: .orbit
    }
  }
}
