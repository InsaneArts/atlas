/// Where the Settings window stands: one of its pages, an account's own page under Accounts, or a desktop's own page
/// under Desktops.
public enum SettingsRoute: Hashable, Sendable {
  /// A page from the sidebar.
  case page(SettingsPage)
  /// The page of the account with this id, pushed over Accounts.
  case account(String)
  /// The page of the desktop with this ID, pushed over Desktops, which stands in Helper's place.
  case desktop(String)

  /// The sidebar's page the route sits under: an account's page sits under Accounts, a desktop's under Desktops.
  public var page: SettingsPage {
    switch self {
    case .page(let page): page
    case .account: .accounts
    case .desktop: .helper
    }
  }

  /// The account the route shows, or nil for any other route.
  public var account: String? {
    switch self {
    case .account(let id): id
    case .page, .desktop: nil
    }
  }

  /// The desktop the route shows, or nil for any other route.
  public var desktop: String? {
    switch self {
    case .desktop(let id): id
    case .page, .account: nil
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

  /// The route once Atlas lists only these desktops: the page of a desktop no longer listed, or while fewer than two
  /// are, goes back to the page in Helper's place; any other route stays.
  ///
  /// - Parameter listed: The IDs of the desktops listed now.
  /// - Returns: The route.
  public func kept(desktops listed: [String]) -> SettingsRoute {
    guard let id = desktop, listed.count < 2 || !listed.contains(id) else { return self }
    return .page(.helper)
  }
}
