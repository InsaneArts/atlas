import AtlasUI

/// Where the window stands: a page chosen in the sidebar, an account's page, Back and Forward, each going through
/// ``SettingsNavigation``. A page is remembered for the next time the window opens, and the page come to reads what it
/// shows afresh, however the window came to it: Integrations its agents, Privacy the person's files.
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
    let before = session.navigation.current
    move(&session.navigation)
    let route = session.navigation.current
    guard route != before else { return }
    arrived(at: route)
  }

  /// The window came to a route: its page is saved, an account's as Accounts, and Integrations reads its agents and
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
