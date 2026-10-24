extension ViewerTab {
  /// The tab's symbol, which a narrow window draws in place of its word.
  public var symbol: SymbolName {
    switch self {
    case .screen: .display
    case .activity: .list
    case .apps: .windows
    case .screenshots: .photos
    }
  }
}
