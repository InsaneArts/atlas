import AtlasService
import AtlasUI

/// Where the window stands: a page chosen in the sidebar or an account's page, and no move while an alert or Sign In to
/// a Site…'s sheet waits for an answer. A page is remembered for the next time the window opens, and the page come to
/// reads what it shows afresh, however the window came to it: Integrations its agents, Privacy the person's files. Each
/// time Atlas reads its accounts list, the page of an account it no longer lists goes back to Accounts.
extension SettingsController {
  /// Whether the window asks something now, in an alert or Sign In to a Site…'s sheet, which holds it where it stands.
  var isAsking: Bool {
    session.alert != nil || signIn != nil
  }

  /// Shows a page from the sidebar, or from another window's link.
  ///
  /// - Parameter page: The page.
  func select(_ page: SettingsPage) {
    go(to: .page(page))
  }

  /// Goes to a route, then saves the page come to and reads what it shows; going to the route shown does nothing.
  ///
  /// - Parameter route: Where to go.
  func go(to route: SettingsRoute) {
    guard !isAsking else { return }
    keepListedAccounts()
    guard route != session.route else { return }
    session.route = route
    arrived(at: route)
  }

  /// The accounts Atlas lists now; nil while it can't read its list.
  var listedAccounts: [ViewerStatus.Account]? {
    dependencies.links.status().status.overview.accounts
  }

  /// Takes in an authoritative reading of the accounts list: the page of an account it no longer lists goes back to
  /// Accounts, and each listed account is kept as listed, so its page keeps its newest name should the list turn
  /// unreadable. While the list can't be read, nothing changes.
  func keepListedAccounts() {
    guard let listed = listedAccounts else { return }
    let route = session.route.kept(accounts: Set(listed.map(\.id)))
    if route != session.route { session.route = route }
    let known = Dictionary(listed.map { ($0.id, $0) }, uniquingKeysWith: { _, last in last })
    if known != session.knownAccounts { session.knownAccounts = known }
  }

  /// Takes in each reading that changes the accounts list, for the controller's whole life, so a removal or a new name
  /// read while the window is closed still holds when it reopens.
  func followAccounts() {
    StateTracking.follow { [weak self] () -> [ViewerStatus.Account]?? in
      guard let self else { return nil }
      return .some(listedAccounts)
    } apply: { [weak self] (_: [ViewerStatus.Account]?) in
      self?.keepListedAccounts()
    }
  }

  /// The window came to a route: its page is saved, an account's as Accounts, Integrations reads its agents and
  /// Privacy the person's files.
  ///
  /// - Parameter route: The route.
  func arrived(at route: SettingsRoute) {
    dependencies.preferences.saveTab(route.page.rawValue)
    switch route {
    case .page(.integrations): readAgents()
    case .page(.privacy): Task { await readFiles() }
    default: break
    }
  }
}
