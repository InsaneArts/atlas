/// The Details pane beside the main pane: the chosen host session, when one is, then the helper, whether it is ready
/// to act, the picture and the helper's login, each a group of rows.
public struct ViewerDetailsState: Equatable, Sendable {
  /// The chosen host session, which leads the pane; nil while every session's work shows, or nothing is known of it.
  public var sessionDetail: ViewerSessionDetail?
  /// The helper's rows: its account, its state and its display.
  public var helper: [Fact]
  /// Ready to act's rows: one while every check passes, else those that do not and a count of those that do.
  public var ready: [ViewerReadyRow]
  /// Whether Check Again is reading the helper now, which its button shows with a spinner.
  public var isChecking: Bool
  /// How often the picture updates, such as "Live".
  public var updates: String
  /// How old the newest picture is, such as "just now", as of the state; the app's window reads it again as the row
  /// draws (``ViewerView``), so the picture's age ticks there alone.
  public var newest: String
  /// The helper's login's rows: when it logged in, and the agents connected to Atlas.
  public var login: [Fact]

  /// Creates the Details column's state.
  ///
  /// - Parameters:
  ///   - sessionDetail: The chosen host session; none by default.
  ///   - helper: The helper's rows.
  ///   - ready: Ready to act's rows.
  ///   - updates: How often the picture updates.
  ///   - newest: How old the newest picture is, as of the state.
  ///   - login: The helper's login's rows.
  ///   - isChecking: Whether Check Again is reading the helper now.
  public init(
    sessionDetail: ViewerSessionDetail? = nil,
    helper: [Fact],
    ready: [ViewerReadyRow],
    updates: String,
    newest: String,
    login: [Fact],
    isChecking: Bool = false
  ) {
    self.sessionDetail = sessionDetail
    self.helper = helper
    self.ready = ready
    self.isChecking = isChecking
    self.updates = updates
    self.newest = newest
    self.login = login
  }
}
