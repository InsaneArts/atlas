/// The app a row acted in, as the log names it: its name, and its bundle identifier when the service saw one. No
/// path, title or document is ever part of it.
public struct ActivityApp: Equatable, Sendable {
  /// The app's name, such as "TextEdit", when the row names one.
  public var name: String?
  /// Its bundle identifier, such as `com.apple.TextEdit`, when the service saw one.
  public var bundleId: String?

  /// Creates an app.
  ///
  /// - Parameters:
  ///   - name: The app's name.
  ///   - bundleId: Its bundle identifier.
  public init(name: String?, bundleId: String? = nil) {
    self.name = name
    self.bundleId = bundleId
  }

  /// The app a row acted in; none for a row that names none, such as a screenshot or a command.
  ///
  /// - Parameter entry: A row of the log.
  init?(_ entry: ActivityLog.Entry) {
    guard entry.app != nil || entry.bundleId != nil else { return nil }
    self.init(name: entry.app, bundleId: entry.bundleId)
  }

  /// Whether `other` is the same app: by bundle identifier when both have one, else by name, ignoring case. Two apps
  /// with nothing in common to compare are not the same.
  ///
  /// - Parameter other: Another app.
  /// - Returns: Whether they are one app.
  func isSame(as other: ActivityApp) -> Bool {
    if let bundleId, let other = other.bundleId { return bundleId == other }
    guard let name, let other = other.name else { return false }
    return name.lowercased() == other.lowercased()
  }

  /// The app with what `other`, the same app, names that it does not: a bundle identifier or a name.
  ///
  /// - Parameter other: The same app, as another row names it.
  /// - Returns: The app, filled in.
  func filled(from other: ActivityApp) -> ActivityApp {
    ActivityApp(name: name ?? other.name, bundleId: bundleId ?? other.bundleId)
  }
}
