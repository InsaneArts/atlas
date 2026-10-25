/// One readiness check, as the viewer's Ready card lists it.
public struct ReadinessItem: Equatable, Sendable, Identifiable {
  /// What is checked, such as "Mouse and keyboard".
  public var name: String
  /// What it means, or what to do.
  public var text: String
  /// How it stands.
  public var state: ReadinessState

  /// Creates a readiness check.
  ///
  /// - Parameters:
  ///   - name: What is checked, such as "Mouse and keyboard".
  ///   - text: What it means, or what to do.
  ///   - state: How it stands.
  public init(name: String, text: String, state: ReadinessState) {
    self.name = name
    self.text = text
    self.state = state
  }

  /// The check's name, which no other check shares.
  public var id: String { name }
}
