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

  /// The route once Atlas lists only these accounts: the page of an account no longer listed goes back to Accounts, any
  /// other route stays.
  ///
  /// - Parameter listed: The ids of the accounts listed now.
  /// - Returns: The route.
  public func kept(accounts listed: Set<String>) -> SettingsRoute {
    guard let id = account, !listed.contains(id) else { return self }
    return .page(.accounts)
  }
}
