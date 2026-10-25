import os

/// The login timing's markers from the person's side, in the unified log beside macOS's own milestones, where `atlas
/// doctor --login-timing` reads them. Each is public and names user IDs and times only, never an account's name.
enum LoginLog {
  /// The log Atlas's login markers go to: the subsystem and category the command line writes too.
  private static let logger = Logger(subsystem: "com.insanearts.atlas", category: "Login")

  /// Notes `marker` in the unified log.
  ///
  /// - Parameter marker: The marker, such as "helper.answering after=8.5s".
  static func mark(_ marker: String) {
    logger.log("\(marker, privacy: .public)")
  }

  /// A time as a marker writes it.
  ///
  /// - Parameter duration: The time.
  /// - Returns: Seconds to one place, such as "8.5s".
  static func seconds(_ duration: Duration) -> String {
    let seconds = Double(duration.components.seconds) + Double(duration.components.attoseconds) / 1e18
    return "\(seconds.formatted(.number.precision(.fractionLength(1)).locale(.init(identifier: "en_US_POSIX"))))s"
  }
}
