extension ViewerStatus {
  /// Whether a macOS service that slows the Mac keeps restarting in the helper, as launchd's log shows. It quits at
  /// once in a login that is not on the screen and macOS starts it again every ten seconds; through a macOS bug, the
  /// restarts slowly slow the whole Mac. Atlas can't turn it off; logging the helper out clears what has built up.
  public enum RestartingService: String, Decodable, Equatable, Sendable {
    /// Nothing that slows the Mac restarts.
    case clear = "none"
    /// It restarts, so the Mac slows while the helper stays logged in.
    case restarting
    /// Atlas could not tell.
    case unknown
  }
}
