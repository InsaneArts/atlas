extension ViewerPlace {
  /// The place's symbol, which its Library row draws before its word.
  public var symbol: SymbolName {
    switch self {
    case .screen: .display
    case .activity: .list
    case .apps: .windows
    case .screenshots: .photos
    }
  }
}
