import AtlasUI

/// Where the window stands: a page chosen in the sidebar, an account's page, Back and Forward, each going through
/// ``SettingsNavigation``, and none while an alert or Sign In to a Site…'s sheet waits for an answer. A page is
/// remembered for the next time the window opens, and the page come to reads what it shows afresh, however the window
/// came to it: Integrations its agents, Privacy the person's files. Each time Atlas reads its accounts list, routes to
/// accounts it no longer lists leave the history for good.
extension SettingsController {
  /// Shows a page from the sidebar, or from another window's link.
  ///
  /// - Parameter page: The page.
  func select(_ page: SettingsPage) {
    navigate { $0.go(to: .page(page)) }
  }

  /// Moves the window's history, then saves the page come to and reads what it shows.
  ///
  /// - Parameter move: The move.
  func navigate(_ move: (inout SettingsNavigation) -> Void) {
    guard session.alert == nil, signIn == nil else { return }
    keepListedAccounts()
    let before = session.navigation.current
    move(&session.navigation)
    let route = session.navigation.current
    guard route != before else { return }
    arrived(at: route)
  }

  /// The accounts Atlas lists now, by id; nil while it can't read its list.
  var listedAccounts: Set<String>? {
    dependencies.links.status().status.overview.accounts
      .map { Set($0.map(\.id)) }
  }

  /// Drops every route to an account Atlas no longer lists from the history, kept so, for a list read later that
  /// can't be read never brings them back; an account's page shown for one goes back to Accounts. While the list can't
  /// be read, the history stays as it is.
  func keepListedAccounts() {
    guard let listed = listedAccounts else { return }
    var navigation = session.navigation
    navigation.keep(accounts: listed)
    if navigation != session.navigation { session.navigation = navigation }
  }

  /// The window came to a route: its page is saved, an account's as Accounts, Integrations reads its agents and
  /// Privacy the person's files, and an account's page keeps the account as listed now, so it keeps its name should
  /// the list turn unreadable.
  ///
  /// - Parameter route: The route.
  func arrived(at route: SettingsRoute) {
    dependencies.preferences.saveTab(route.page.rawValue)
    switch route {
    case .page(.integrations): readAgents()
    case .page(.privacy): Task { await readFiles() }
    case .account(let id):
      let listed = dependencies.links.status().status.overview.accounts?
        .first { $0.id == id }
      if let listed { session.knownAccounts[id] = listed }
    default: break
    }
  }
}
