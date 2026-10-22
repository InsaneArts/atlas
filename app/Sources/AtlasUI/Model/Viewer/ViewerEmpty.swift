/// Why the screen shows no picture, in place of one, and what its one button sends.
package struct ViewerEmpty: Equatable, Sendable {
  /// What the message says.
  package var message: EmptyMessageState
  /// What its button sends; nil when it has none.
  package var event: ViewerEvent?

  /// Creates the screen's empty message.
  ///
  /// - Parameters:
  ///   - message: What the message says.
  ///   - event: What its button sends; nil when it has none.
  package init(message: EmptyMessageState, event: ViewerEvent? = nil) {
    self.message = message
    self.event = event
  }
}
