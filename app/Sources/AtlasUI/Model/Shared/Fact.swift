/// A name and its value, for a card of facts.
package struct Fact: Equatable, Sendable, Identifiable {
  /// What the fact is, such as "Display".
  package var name: String
  /// Its value in words.
  package var value: String

  /// Creates a fact.
  ///
  /// - Parameters:
  ///   - name: What the fact is, such as "Display".
  ///   - value: Its value in words.
  package init(name: String, value: String) {
    self.name = name
    self.value = value
  }

  /// The fact's name, which no other fact in its card shares.
  package var id: String { name }
}
