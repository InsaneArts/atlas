/// The Settings shell, its four existing content panes and its window-wide alert.
public struct SettingsState: Equatable, Sendable {
  /// Labels and selection for the existing content panes.
  public var tabs: CapsuleTabsState<SettingsTab>
  /// Whether history has an earlier destination.
  public var canGoBack: Bool
  /// Whether history has a later destination.
  public var canGoForward: Bool
  /// The General pane.
  public var general: SettingsGeneralState
  /// The Helper pane.
  public var helper: SettingsHelperState
  /// The Agents pane.
  public var agents: SettingsAgentsState
  /// The About pane.
  public var about: SettingsAboutState
  /// The alert over the whole window, if one is asked.
  public var alert: AlertCardState<AlertChoice>?

  /// Creates the window's state.
  ///
  /// - Parameters:
  ///   - tabs: The tab row, with the pane shown selected.
  ///   - general: The General pane.
  ///   - helper: The Helper pane.
  ///   - agents: The Agents pane.
  ///   - about: The About pane.
  ///   - alert: The alert over the whole window, if one is asked.
  ///   - canGoBack: Whether history has an earlier destination.
  ///   - canGoForward: Whether history has a later destination.
  public init(
    tabs: CapsuleTabsState<SettingsTab>,
    general: SettingsGeneralState,
    helper: SettingsHelperState,
    agents: SettingsAgentsState,
    about: SettingsAboutState,
    alert: AlertCardState<AlertChoice>? = nil,
    canGoBack: Bool = false,
    canGoForward: Bool = false
  ) {
    self.canGoBack = canGoBack
    self.canGoForward = canGoForward
    self.tabs = tabs
    self.general = general
    self.helper = helper
    self.agents = agents
    self.about = about
    self.alert = alert
  }

  /// The selected pane's name, which the title bar shows.
  public var title: String {
    tabs.tabs.first { $0.id == tabs.selected }?.title ?? ""
  }
}
