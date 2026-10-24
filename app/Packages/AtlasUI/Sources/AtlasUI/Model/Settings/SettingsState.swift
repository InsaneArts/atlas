/// The Settings window: its tab row, its four panes and the alert over it. The selected pane lives only in the tabs.
public struct SettingsState: Equatable, Sendable {
  /// The tab row, with the pane shown selected.
  public var tabs: IconTabsState<SettingsTab>
  /// The General pane.
  public var general: SettingsGeneralState
  /// The Helper pane.
  public var helper: SettingsHelperState
  /// The Agents pane.
  public var agents: SettingsAgentsState
  /// The About pane.
  public var about: SettingsAboutState
  /// The alert over the whole window, if one is asked.
  public var alert: OrbitAlertState<AlertChoice>?

  /// Creates the window's state.
  ///
  /// - Parameters:
  ///   - tabs: The tab row, with the pane shown selected.
  ///   - general: The General pane.
  ///   - helper: The Helper pane.
  ///   - agents: The Agents pane.
  ///   - about: The About pane.
  ///   - alert: The alert over the whole window, if one is asked.
  public init(
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
