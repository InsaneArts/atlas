import AtlasService

/// When the menu bar panel tells the person, once, that a macOS service in the helper slowly slows their Mac and that
/// logging the helper out and back in clears what built up: while the service is seen restarting, until the
/// person says OK.
public enum SlowServiceNote {
  /// Whether the panel shows the note now.
  ///
  /// - Parameters:
  ///   - status: The service's latest status.
  ///   - dismissed: Whether the person already said OK to it.
  /// - Returns: Whether it shows.
  public static func shows(_ status: ViewerStatus, dismissed: Bool) -> Bool {
    !dismissed && status.restartingService == .restarting
  }
}
