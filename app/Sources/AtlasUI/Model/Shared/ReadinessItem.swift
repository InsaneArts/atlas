/// One readiness check, as the viewer's Ready card lists it.
package struct ReadinessItem: Equatable, Sendable, Identifiable {
  /// What is checked, such as "Input service".
  package var name: String
  /// What it means, or what to do.
  package var text: String
  /// How it stands.
  package var state: ReadinessState

  /// Creates a readiness check.
  ///
  /// - Parameters:
  ///   - name: What is checked, such as "Input service".
  ///   - text: What it means, or what to do.
  ///   - state: How it stands.
  package init(name: String, text: String, state: ReadinessState) {
    self.name = name
    self.text = text
    self.state = state
  }

  /// The check's name, which no other check shares.
  package var id: String { name }
}
