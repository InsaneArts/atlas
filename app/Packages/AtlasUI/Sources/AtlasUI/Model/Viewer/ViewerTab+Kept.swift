extension ViewerTab {
  /// The tabs whose panes the window keeps built once shown, in the bar's order: every tab but the Screen, whose
  /// picture and input surface exist only while it shows.
  public static let keptTabs = allCases.filter(\.isKept)

  /// Whether the window keeps this tab's pane built once shown, moving and fading it as the tab changes rather than
  /// building it again.
  public var isKept: Bool {
    self != .screen
  }
}
