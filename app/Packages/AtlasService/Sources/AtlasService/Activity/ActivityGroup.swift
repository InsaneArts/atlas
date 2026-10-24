/// The lines the Activity tab lists under one stretch of time, newest first.
public struct ActivityGroup: Equatable, Sendable, Identifiable {
  /// The stretch of time.
  public var period: ActivityPeriod
  /// Its lines, newest first.
  public var runs: [ActivityRun]

  /// The group's identity: its stretch of time, which no other group shares.
  public var id: ActivityPeriod { period }

  /// Creates a group.
  ///
  /// - Parameters:
  ///   - period: The stretch of time.
  ///   - runs: Its lines, newest first.
  init(period: ActivityPeriod, runs: [ActivityRun]) {
    self.period = period
    self.runs = runs
  }
}
