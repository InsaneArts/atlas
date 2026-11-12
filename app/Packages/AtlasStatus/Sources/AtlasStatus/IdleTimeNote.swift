import AtlasService

/// The menu bar panel's note that the person's apps think they are away: it shows while an app keeps the idle time
/// their apps read stopped, until the person says OK. A stop with no app to quit shows no note, since the person has no
/// step to take.
public struct IdleTimeNote: Equatable, Sendable {
  /// The app keeping secure keyboard entry on, by the name it shows; nil when it has none.
  public var app: String?

  /// The note while the panel shows it, or nil while it does not: no app keeps the idle time stopped, or the person
  /// said OK.
  ///
  /// - Parameters:
  ///   - status: The service's latest status.
  ///   - dismissed: Whether the person said OK to it since the idle time last stopped.
  /// - Returns: The note, or nil.
  public static func shown(_ status: ViewerStatus, dismissed: Bool) -> IdleTimeNote? {
    let idleTime = status.idleTime
    guard !dismissed, idleTime.state == .held else { return nil }
    return IdleTimeNote(app: idleTime.app.flatMap { $0.isEmpty ? nil : $0 })
  }
}
