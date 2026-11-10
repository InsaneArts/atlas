/// Where the Settings window stands and where it has been, as a browser keeps it: the route shown, the routes Back
/// returns to, the nearest last, and the routes Forward goes to again, the nearest last. Pure value changes, so the
/// window's controller keeps it and tests read it.
public struct SettingsNavigation: Equatable, Sendable {
  /// The route shown.
  public private(set) var current: SettingsRoute
  /// The routes Back returns to, the nearest last.
  public private(set) var back: [SettingsRoute]
  /// The routes Forward goes to again, the nearest last.
  public private(set) var forward: [SettingsRoute]

  /// Creates the history standing on a route, with nothing behind or ahead.
  ///
  /// - Parameter current: The route shown.
  public init(_ current: SettingsRoute = .page(.helper)) {
    self.current = current
    back = []
    forward = []
  }

  /// Whether Back has somewhere to go.
  public var canGoBack: Bool { !back.isEmpty }
  /// Whether Forward has somewhere to go.
  public var canGoForward: Bool { !forward.isEmpty }

  /// Goes to a route, keeping the one shown for Back and dropping what Forward held, as a browser does; going to the
  /// route shown does nothing.
  ///
  /// - Parameter route: Where to go.
  public mutating func go(to route: SettingsRoute) {
    guard route != current else { return }
    back.append(current)
    forward.removeAll()
    current = route
  }

  /// Returns to the last route left, keeping the one shown for Forward; nothing without one.
  public mutating func goBack() {
    guard let previous = back.popLast() else { return }
    forward.append(current)
    current = previous
  }

  /// Goes again to the last route Back left, keeping the one shown for Back; nothing without one.
  public mutating func goForward() {
    guard let next = forward.popLast() else { return }
    back.append(current)
    current = next
  }

  /// Drops every route to an account no longer listed, from both stacks; an account's page shown for one goes back to
  /// Accounts without keeping it for Forward. Two routes left side by side by the drop fold into one.
  ///
  /// - Parameter listed: The ids of the accounts listed now.
  public mutating func keep(accounts listed: Set<String>) {
    let isGone: (SettingsRoute) -> Bool = { route in
      route.account.map { !listed.contains($0) } ?? false
    }
    back = Self.folded(back.filter { !isGone($0) })
    forward = Self.folded(forward.filter { !isGone($0) })
    if isGone(current) { current = .page(.accounts) }
    if back.last == current { back.removeLast() }
    if forward.last == current { forward.removeLast() }
  }

  /// A stack with neighbours that are the same route folded into one.
  ///
  /// - Parameter routes: The stack.
  /// - Returns: The stack, no route twice in a row.
  private static func folded(_ routes: [SettingsRoute]) -> [SettingsRoute] {
    routes.reduce(into: []) { kept, route in
      if kept.last != route { kept.append(route) }
    }
  }
}
