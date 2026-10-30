/// The host session that opened an app, as its card names it while every session's apps show: the host's icon and
/// the session's name.
public struct ViewerAppOrigin: Equatable, Sendable {
  /// The host's icon; nil for a session nothing names.
  public var icon: RowIcon?
  /// The session's name, such as "Claude Code · understudy".
  public var name: String

  /// Creates an origin.
  ///
  /// - Parameters:
  ///   - icon: The host's icon.
  ///   - name: The session's name.
  public init(icon: RowIcon?, name: String) {
    self.icon = icon
    self.name = name
  }
}
