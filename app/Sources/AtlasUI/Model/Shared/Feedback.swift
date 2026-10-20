/// What the last action said, and whether it still runs.
package struct Feedback: Equatable, Sendable {
  /// The words.
  package var text: String
  /// Whether the action still runs; a spinner shows beside the words while it does.
  package var isWorking: Bool

  /// Creates what an action said.
  ///
  /// - Parameters:
  ///   - text: The words.
  ///   - isWorking: Whether the action still runs; a spinner shows beside the words while it does.
  package init(text: String, isWorking: Bool) {
    self.text = text
    self.isWorking = isWorking
  }
}
