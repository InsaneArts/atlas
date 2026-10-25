/// A login Atlas can close, which the Helper pane lists above its checks: a deleted account's login still open, or the
/// helper's own while Atlas there is silent.
public struct SettingsCloseLoginRow: Equatable, Sendable, Identifiable {
  /// What is open, such as "A deleted account is still logged in".
  public var title: String
  /// Why it matters.
  public var detail: String
  /// The button's title, such as "Close It…".
  public var button: String
  /// What the button sends.
  public var event: SettingsEvent
  /// Whether the button can run now: nothing else runs.
  public var canRun: Bool
  /// The line under the row while Atlas closes it; nil otherwise.
  public var status: String?

  /// Creates the row.
  ///
  /// - Parameters:
  ///   - title: What is open.
  ///   - detail: Why it matters.
  ///   - button: The button's title.
  ///   - event: What the button sends.
  ///   - canRun: Whether the button can run now.
  ///   - status: The line under the row while Atlas closes it; nil by default.
  public init(
    title: String,
    detail: String,
    button: String,
    event: SettingsEvent,
    canRun: Bool,
    status: String? = nil
  ) {
    self.title = title
    self.detail = detail
    self.button = button
    self.event = event
    self.canRun = canRun
    self.status = status
  }

  /// The pane holds one such row.
  public var id: String { title }
}
