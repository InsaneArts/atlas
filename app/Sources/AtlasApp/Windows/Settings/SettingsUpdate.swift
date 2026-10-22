import AtlasKit

/// Where a check for updates stands in About: one check or install runs at a time.
enum SettingsUpdate: Equatable {
  /// Nothing asked yet.
  case idle
  /// A check runs.
  case checking
  /// The last check answered.
  case answered(UpdateAnswer)
  /// This newer version installs.
  case installing(String)
  /// The newer version installed.
  case installed
  /// The install failed, saying why.
  case installFailed(String)

  /// Whether a check or an install runs now, so neither starts again.
  var isRunning: Bool {
    switch self {
    case .checking, .installing: true
    case .idle, .answered, .installed, .installFailed: false
    }
  }

  /// The newer version the last check found, which Install offers.
  var newer: String? {
    guard case .answered(.newer(let version)) = self else { return nil }
    return version
  }
}
