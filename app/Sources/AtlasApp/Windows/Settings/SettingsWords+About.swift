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
  /// No release is published yet.
  static let unpublished = "Atlas has no public release yet. Check atlas.insanearts.io for news."
  /// Once the newer version is installed, in the moment before Atlas restarts itself to finish.
  static let installed = "Atlas is updated and restarts itself to finish."

  /// The check found a newer version.
  ///
  /// - Parameter version: The newer version.
  /// - Returns: "Atlas 0.2.0 is available."
  static func available(_ version: String) -> String {
    "Atlas \(version) is available."
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
