extension ViewerPane {
  /// The place that shows this pane.
  public var place: ViewerPlace {
    switch self {
    case .screen: .screen
    case .activity: .activity
    case .apps: .apps
    case .screenshots: .screenshots
    }
  }
}
