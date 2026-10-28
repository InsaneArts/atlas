/// Whose work the main window shows: every host session's, or one's.
public enum ViewerSessionChoice: Hashable, Sendable {
  /// Every session's work, the person's own included.
  case all
  /// One host session's work, by its key; the empty key is Terminal's, the commands no host named.
  ///
  /// - Parameter key: The session's key.
  case one(String)
}
