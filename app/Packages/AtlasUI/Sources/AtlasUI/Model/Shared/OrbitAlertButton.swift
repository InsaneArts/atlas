/// One button of an alert: what it sends, its word and its role, which sets its look and its key.
public struct OrbitAlertButton<ID: Hashable & Sendable>: Equatable, Sendable, Identifiable {
  /// What a button does, which sets its look and its key.
  public enum Role: Sendable {
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
  public var id: ID
  /// The button's word, such as "Recover".
  public var title: String
  /// What it does.
  public var role: Role

  /// Creates an alert's button.
  ///
  /// - Parameters:
  ///   - id: What pressing it sends.
  ///   - title: The button's word.
  ///   - role: What it does.
  public init(id: ID, title: String, role: Role) {
    self.id = id
    self.title = title
    self.role = role
  }
}
