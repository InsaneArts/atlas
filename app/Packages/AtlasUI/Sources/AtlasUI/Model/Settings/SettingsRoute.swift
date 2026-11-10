/// Where the Settings window stands: one of its pages, or an account's own page under Accounts.
public enum SettingsRoute: Hashable, Sendable {
  /// A page from the sidebar.
  case page(SettingsPage)
  /// The page of the account with this id, pushed over Accounts.
  case account(String)

  /// The sidebar's page the route sits under: an account's page sits under Accounts.
  public var page: SettingsPage {
    switch self {
    case .page(let page): page
    case .account: .accounts
    }
  }

  /// The account the route shows, or nil for a page.
  public var account: String? {
    switch self {
    case .page: nil
    case .account(let id): id
    }
  }
}
