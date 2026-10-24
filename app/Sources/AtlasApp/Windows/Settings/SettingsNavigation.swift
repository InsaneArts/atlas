import AtlasUI

/// Transient visits and their cursor. Navigation has no service, preference or command dependencies.
struct SettingsNavigation: Equatable {
  /// Visits in navigation order, including any forward branch.
  private(set) var entries: [SettingsDestination]
  /// The current entry's index, or nil before the first visit.
  private(set) var cursor: Int?

  /// Creates an empty history or starts at one restored destination.
  ///
  /// - Parameter initial: The initial destination, if already known.
  init(initial: SettingsDestination? = nil) {
    entries = initial.map { [$0] } ?? []
    cursor = initial == nil ? nil : 0
  }

  /// The current visit, if any.
  var destination: SettingsDestination? { cursor.map { entries[$0] } }
  /// Whether there is an earlier visit.
  var canGoBack: Bool { cursor.map { $0 > 0 } ?? false }
  /// Whether there is a later visit.
  var canGoForward: Bool { cursor.map { $0 + 1 < entries.count } ?? false }

  /// Visits a different location, discarding the forward branch after Back.
  ///
  /// - Parameter destination: The location to visit.
  mutating func visit(_ destination: SettingsDestination) {
    guard destination != self.destination else { return }
    if let cursor { entries.removeSubrange((cursor + 1)..<entries.count) }
    entries.append(destination)
    cursor = entries.count - 1
  }

  /// Moves to an earlier visit without adding an entry.
  mutating func back() {
    guard canGoBack, let cursor else { return }
    self.cursor = cursor - 1
  }

  /// Moves to a later visit without adding an entry.
  mutating func forward() {
    guard canGoForward, let cursor else { return }
    self.cursor = cursor + 1
  }
}
