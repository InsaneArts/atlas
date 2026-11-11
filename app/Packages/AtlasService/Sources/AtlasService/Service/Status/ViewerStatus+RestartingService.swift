extension ViewerStatus {
  /// Whether a macOS service that slows the Mac restarted over and over in the helper lately, as launchd's log shows.
  /// It quits at once in a login that is not on the screen and macOS starts it again every ten seconds; through a
  /// macOS bug, the restarts slowly slow the whole Mac. Atlas tries to keep it paused while the helper is hidden;
  /// logging the helper out clears what built up.
  public enum RestartingService: String, Decodable, Equatable, Sendable {
    /// Nothing that slows the Mac restarted often in the last hour.
    case clear = "none"
    /// It restarted often in the last hour: before Atlas paused it, or because Atlas could not.
    case restarting
    /// Atlas could not tell.
    case unknown
  }
}
