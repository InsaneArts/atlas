/// The Details pane beside the main pane: the helper, whether it is ready to act, the picture and the session, each a
/// group of rows.
package struct ViewerDetailsState: Equatable, Sendable {
  /// The helper's rows: its account, its state and its display.
  package var helper: [Fact]
  /// Ready to act's rows: one while every check passes, else those that do not and a count of those that do.
  package var ready: [ViewerReadyRow]
  /// Whether Check Again is reading the helper now, which its button shows with a spinner.
  package var isChecking: Bool
  /// How often the picture updates, such as "Live".
  package var updates: String
  /// How old the newest picture is, such as "just now", as of the state; the app's window reads it again as the row
  /// draws (``ViewerView``), so the picture's age ticks there alone.
  package var newest: String
  /// The session's rows.
  package var session: [Fact]

  /// Creates the Details column's state.
  ///
  /// - Parameters:
  ///   - helper: The helper's rows.
  ///   - ready: Ready to act's rows.
  ///   - updates: How often the picture updates.
  ///   - newest: How old the newest picture is, as of the state.
  ///   - session: The session's rows.
  ///   - isChecking: Whether Check Again is reading the helper now.
  package init(
    helper: [Fact],
    ready: [ViewerReadyRow],
    updates: String,
    newest: String,
    session: [Fact],
    isChecking: Bool = false
  ) {
    self.helper = helper
    self.ready = ready
    self.isChecking = isChecking
    self.updates = updates
    self.newest = newest
    self.session = session
  }
}
