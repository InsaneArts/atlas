import AtlasService

/// When the panel names another account at the person's screen.
public enum WrongAccountNote {
  /// The name to show while macOS names another account, until dismissed.
  ///
  /// - Parameters:
  ///   - status: The latest status.
  ///   - dismissed: Whether the person said OK.
  /// - Returns: The other account's name, or nil to hide the note.
  public static func name(_ status: ViewerStatus, dismissed: Bool) -> String? {
    guard !dismissed, status.consoleName.state == .another,
      let name = status.consoleName.name, !name.isEmpty
    else { return nil }
    return name
  }
}
