/// Whose work a moment was, as a moment names it while every session's work shows: the host session's icon and
/// name, or the person's own mark and "You".
public struct ViewerMomentSession: Equatable, Sendable {
  /// The host's icon; nil for the person's own, which their mark leads.
  public var icon: RowIcon?
  /// The session's name, such as "Claude Code · understudy", or "You".
  public var name: String

  /// Creates a moment's session.
  ///
  /// - Parameters:
  ///   - icon: The host's icon; nil for the person's own.
  ///   - name: The session's name.
  public init(icon: RowIcon?, name: String) {
    self.icon = icon
    self.name = name
  }
}
