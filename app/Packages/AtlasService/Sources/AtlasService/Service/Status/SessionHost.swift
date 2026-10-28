/// The app a host session runs in, as the service names it. A name this app does not know, or one a session gave
/// itself, reads as ``other``.
public enum SessionHost: String, Equatable, Sendable {
  /// Claude Code.
  case claude
  /// Codex.
  case codex
  /// Cursor.
  case cursor
  /// opencode.
  case opencode
  /// Pi.
  case pi
  /// A host this app does not know.
  case other
  /// No host: the commands no host named, gathered as Terminal.
  case terminal = "none"

  /// The host the service's word names.
  ///
  /// - Parameter word: The session's `host`, or nil when it names none.
  init(word: String?) {
    self = word.flatMap(Self.init(rawValue:)) ?? .other
  }
}
