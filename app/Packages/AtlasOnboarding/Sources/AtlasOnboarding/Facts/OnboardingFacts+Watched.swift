import Foundation

extension OnboardingFacts {
  /// The folders whose changes may move setup on, which the window watches between readings: in the person's
  /// account, the helper's acceptance folder, where the helper's Allow is written; in the helper, its job's folder,
  /// where the job's socket comes and goes as it starts and restarts. None before a reading names the helper.
  ///
  /// - Parameter home: This account's home folder.
  /// - Returns: The folders' paths.
  public func watchedFolders(home: String) -> [String] {
    switch role {
    case .controller: helper.uid > 0 ? ["/Users/Shared/Atlas-accept-\(helper.uid)"] : []
    case .helper: [URL(fileURLWithPath: home).appending(path: ".atlas/helper").path]
    }
  }
}
