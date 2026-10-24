/// The main window's own alert's words: restart, which only the main window asks, with its title, sentence and
/// buttons. The alerts every window asks read from ``HelperWords``.
extension ViewerWords {
  /// The restart alert's title.
  static let restartTitle = "Restart the helper?"
  /// The restart alert's sentence.
  static let restartMessage = "Atlas starts its service in the helper again. Anything an agent is doing there stops."
  /// The restart alert's button.
  static let restartButton = "Restart"
  /// The restart alert's button while it runs.
  static let restarting = "Restarting…"
}
