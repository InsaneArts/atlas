/// One choice of a popup button: the value it stands for, its word, and whether it can be chosen now.
package struct PopupOption<Value: Hashable & Sendable>: Equatable, Sendable, Identifiable {
  /// The value choosing it sets, which identifies the choice.
  package var id: Value
  /// The choice's word, such as "Live".
  package var title: String
  /// Whether it can be chosen now.
  package var isEnabled: Bool

  /// Creates a choice.
  ///
  /// - Parameters:
  ///   - id: The value choosing it sets.
  ///   - title: The choice's word.
  ///   - isEnabled: Whether it can be chosen now.
  package init(id: Value, title: String, isEnabled: Bool = true) {
    self.id = id
    self.title = title
    self.isEnabled = isEnabled
  }
}
