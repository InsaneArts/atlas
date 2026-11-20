extension Mode.Kind {
  /// How much the row asks of the person, most first: needing them, logged out, not ready, stopped, their control,
  /// working, in use elsewhere, idle, then connecting, which says nothing yet. The menu bar's dot shows the most urgent
  /// row of every desktop.
  public var urgency: Int {
    switch self {
    case .needsYou: 8
    case .loggedOut: 7
    case .notReady: 6
    case .stopped: 5
    case .control: 4
    case .working: 3
    case .inUseElsewhere: 2
    case .idle: 1
    case .connecting: 0
    }
  }

  /// The most urgent of `kinds`, the first of them when two are as urgent.
  ///
  /// - Parameter kinds: Rows, such as every desktop's.
  /// - Returns: The most urgent, or nil when there are none.
  public static func mostUrgent(_ kinds: some Sequence<Mode.Kind>) -> Mode.Kind? {
    kinds.reduce(nil) { most, kind in
      guard let most, most.urgency >= kind.urgency else { return kind }
      return most
    }
  }
}
