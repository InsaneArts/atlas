import AtlasService
import AtlasUI

/// Where the window stands: a page chosen in the sidebar, an account's page, Back and Forward, each going through
/// ``SettingsNavigation``, and none while an alert or Sign In to a Site…'s sheet waits for an answer. A page is
/// remembered for the next time the window opens, and the page come to reads what it shows afresh, however the window
/// came to it: Integrations its agents, Privacy the person's files. Each time Atlas reads its accounts list, routes to
/// accounts it no longer lists leave the history for good.
extension SettingsController {
  /// Whether the window asks something now, in an alert or Sign In to a Site…'s sheet, which holds it where it stands.
  var isAsking: Bool {
    session.alert != nil || signIn != nil
  }

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
    guard !isAsking else { return }
    keepListedAccounts()
    let before = session.navigation.current
    move(&session.navigation)
    let route = session.navigation.current
    guard route != before else { return }
    arrived(at: route)
  }

  /// The accounts Atlas lists now; nil while it can't read its list.
  var listedAccounts: [ViewerStatus.Account]? {
    dependencies.links.status().status.overview.accounts
  }

  /// Takes in an authoritative reading of the accounts list: every route to an account it no longer lists leaves the
  /// history for good, so a list read later that can't be read never brings them back, and an account's page shown
  /// for one goes back to Accounts; and each listed account is kept as listed, so its page keeps its newest name
  /// should the list turn unreadable. While the list can't be read, nothing changes.
  func keepListedAccounts() {
    guard let listed = listedAccounts else { return }
    var navigation = session.navigation
    navigation.keep(accounts: Set(listed.map(\.id)))
    if navigation != session.navigation { session.navigation = navigation }
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
