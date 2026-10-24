import AtlasService

/// How a check for updates ended: Atlas is up to date, a newer version is published, or the check failed.
public enum UpdateAnswer: Equatable, Sendable {
  /// The version installed is the newest.
  case upToDate
  /// A newer version is published: its version, such as `0.2.0`.
  case newer(String)
  /// Atlas has no public release yet, so there is nothing to compare with.
  case unpublished
  /// The check did not answer: it failed, or its reply says neither.
  case failed

  /// The command line's words for a release not published yet, which it gives under the general `update` code.
  static let unpublishedMessage = "Atlas has no public release yet"

  /// The answer `atlas update --check` gave. Only a reply that worked and says whether the newest is newer counts;
  /// a newer one must name its version, which Install asks about. A release not published yet is its own answer.
  ///
  /// - Parameter outcome: The command's outcome.
  /// - Returns: Up to date, newer with its version, unpublished, or failed.
  public static func of(_ outcome: Outcome) -> UpdateAnswer {
    if !outcome.ok, outcome.code == "update", outcome.message.hasPrefix(unpublishedMessage) { return .unpublished }
    guard outcome.ok, let check = outcome.decode(UpdateCheck.self), let newer = check.newer else { return .failed }
    guard newer else { return .upToDate }
    guard let latest = check.latest, !latest.isEmpty else { return .failed }
    return .newer(latest)
  }
}
