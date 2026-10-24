extension ViewerPane {
  /// The tab that shows this pane.
  public var tab: ViewerTab {
    switch self {
    case .screen: .screen
    case .activity: .activity
    case .apps: .apps
    case .screenshots: .screenshots
    }
  }
}
