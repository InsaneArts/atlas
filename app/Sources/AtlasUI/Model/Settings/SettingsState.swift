/// The Settings window: its tab row, its four panes and the alert over it. The selected pane lives only in the tabs.
package struct SettingsState: Equatable, Sendable {
  /// The tab row, with the pane shown selected.
  package var tabs: IconTabsState<SettingsTab>
  /// The General pane.
  package var general: SettingsGeneralState
  /// The Helper pane.
  package var helper: SettingsHelperState
  /// The Agents pane.
  package var agents: SettingsAgentsState
  /// The About pane.
  package var about: SettingsAboutState
  /// The alert over the whole window, if one is asked.
  package var alert: OrbitAlertState<AlertChoice>?

  /// Creates the window's state.
  ///
  /// - Parameters:
  ///   - tabs: The tab row, with the pane shown selected.
  ///   - general: The General pane.
  ///   - helper: The Helper pane.
  ///   - agents: The Agents pane.
  ///   - about: The About pane.
  ///   - alert: The alert over the whole window, if one is asked.
  package init(
    tabs: IconTabsState<SettingsTab>,
    general: SettingsGeneralState,
    helper: SettingsHelperState,
    agents: SettingsAgentsState,
    about: SettingsAboutState,
    alert: OrbitAlertState<AlertChoice>? = nil
  ) {
    self.tabs = tabs
    self.general = general
    self.helper = helper
    self.agents = agents
    self.about = about
    self.alert = alert
  }
}
