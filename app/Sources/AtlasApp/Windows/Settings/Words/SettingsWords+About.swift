/// The About tab's words: the version, the update check's answers, and installing an update, with its alert.
extension SettingsWords {
  /// The version line, or what a build with no version is.
  ///
  /// - Parameter version: The app's version, when known.
  /// - Returns: "Version 0.1.0", or "Development build".
  static func version(_ version: String?) -> String {
    version.map { "Version \($0)" } ?? "Development build"
  }

  /// The check found nothing newer.
  static let upToDate = "Atlas is up to date."
  /// The check did not answer.
  static let checkFailed = "Atlas couldn't check for updates."
  /// No release is published yet, its site a Markdown link, which About shows live.
  static let unpublished =
    "Atlas has no public release yet. Check [atlas.insanearts.io](https://atlas.insanearts.io) for news."
  /// A check runs, under the button, which holds no spinner of its own.
  static let checkingForUpdates = "Checking for updates…"
  /// The install stopped because Atlas isn't where the helper account can run it.
  static let moveToApplications = "Move Atlas to the Applications folder, then try again."

  /// Whether an install failed because Atlas isn't where the helper account can run it, as `atlas update` says.
  ///
  /// - Parameter message: The install's failure.
  /// - Returns: Whether Atlas must move to the Applications folder first.
  static func isMisplaced(_ message: String) -> Bool {
    message.contains("where the helper account can run it")
  }
  /// Once the newer version is installed, in the moment before Atlas restarts itself to finish.
  static let installed = "Atlas is updated and restarts itself to finish."

  /// The check found a newer version.
  ///
  /// - Parameter version: The newer version.
  /// - Returns: "Atlas 0.2.0 is available."
  static func available(_ version: String) -> String {
    "Atlas \(version) is available."
  }

  /// The newer version installing, under About's progress bar.
  ///
  /// - Parameter version: The newer version.
  /// - Returns: "Installing Atlas 0.2.0…".
  static func installingVersion(_ version: String) -> String {
    "Installing Atlas \(version)…"
  }

  /// The Install alert's title.
  ///
  /// - Parameter version: The newer version.
  /// - Returns: "Install Atlas 0.2.0?".
  static func installTitle(_ version: String) -> String {
    "Install Atlas \(version)?"
  }

  /// The Install alert's sentence: the update restarts Atlas, closing this window.
  static let installMessage = "Atlas checks the release's signature and installs it, then restarts itself to finish."
  /// The Install alert's button.
  static let install = "Install"
  /// The Install alert's button while it runs.
  static let installing = "Installing…"
}
