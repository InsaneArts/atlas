extension SettingsPage {
  /// The symbol on the page's tile in the sidebar.
  var symbol: SymbolName {
    switch self {
    case .helper: .display
    case .general: .gear
    case .activity: .clock
    case .accounts: .person
    case .privacy: .lockFilled
    case .integrations: .link
    case .about: .infoLetter
    }
  }

  /// The tile's tint, as a source list colours its rows' icons.
  var tint: MonogramTint {
    switch self {
    case .helper, .general, .about: .graphite
    case .activity: .coral
    case .accounts: .blue
    case .privacy: .indigo
    case .integrations: .pink
    }
  }
}
