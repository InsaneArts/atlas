extension SettingsPage {
  /// The page's symbol in the sidebar, drawn alone in the sidebar's text colour: a gear, a clock turning back, a
  /// person, a lock, a puzzle piece and an "i".
  var symbol: SymbolName {
    switch self {
    case .helper: .display
    case .general: .gear
    case .activity: .history
    case .accounts: .personCircle
    case .privacy: .lock
    case .integrations: .extensions
    case .about: .info
    }
  }
}
