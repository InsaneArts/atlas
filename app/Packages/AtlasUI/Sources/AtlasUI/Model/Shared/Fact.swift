/// A name and its value, for a card of facts.
public struct Fact: Equatable, Sendable, Identifiable {
  /// What the fact is, such as "Display".
  public var name: String
  /// Its value in words.
  public var value: String

  /// Creates a fact.
  ///
  /// - Parameters:
  ///   - name: What the fact is, such as "Display".
  ///   - value: Its value in words.
  public init(name: String, value: String) {
    self.name = name
    self.value = value
  }

  /// The fact's name, which no other fact in its card shares.
  public var id: String { name }
}
