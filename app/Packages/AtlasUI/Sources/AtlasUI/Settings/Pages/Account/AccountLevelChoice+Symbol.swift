extension AccountLevelChoice {
  /// The symbol on the level's tile: an eye, a question in a bubble, a checked shield.
  var symbol: SymbolName {
    switch self {
    case .lookOnly: .eye
    case .ask: .askFirst
    case .trusted: .trusted
    }
  }
}
