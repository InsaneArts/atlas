/// One button of an alert: what it sends, its word and its role, which sets its look and its key.
package struct OrbitAlertButton<ID: Hashable & Sendable>: Equatable, Sendable, Identifiable {
  /// What a button does, which sets its look and its key.
  package enum Role: Sendable {
    /// The action a plain alert asks for: blue, first, and Return presses it.
    case action
    /// The way out beside an action: quiet, second, and Escape presses it.
    case cancel
    /// The way out of a destructive alert, which comes first in blue: Return and Escape both press it.
    case safeCancel
    /// The action a destructive alert asks for: red, second, and no key presses it.
    case destructive
  }

  /// What pressing it sends, which identifies the button.
  package var id: ID
  /// The button's word, such as "Recover".
  package var title: String
  /// What it does.
  package var role: Role

  /// Creates an alert's button.
  ///
  /// - Parameters:
  ///   - id: What pressing it sends.
  ///   - title: The button's word.
  ///   - role: What it does.
  package init(id: ID, title: String, role: Role) {
    self.id = id
    self.title = title
    self.role = role
  }
}
