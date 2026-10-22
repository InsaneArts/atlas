/// The one button an empty message may offer: its word and whether it is the next step.
package struct EmptyMessageAction: Equatable, Sendable {
  /// The button's word, such as "Recover…".
  package var title: String
  /// Whether it is the one thing to do next, drawn white; else it is drawn as glass.
  package var isPrimary: Bool

  /// Creates an empty message's button.
  ///
  /// - Parameters:
  ///   - title: The button's word.
  ///   - isPrimary: Whether it is the one thing to do next.
  package init(title: String, isPrimary: Bool) {
    self.title = title
    self.isPrimary = isPrimary
  }
}
