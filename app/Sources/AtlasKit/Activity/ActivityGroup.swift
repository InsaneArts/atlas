/// The lines the Activity tab lists under one stretch of time, newest first.
package struct ActivityGroup: Equatable, Sendable, Identifiable {
  /// The stretch of time.
  package var period: ActivityPeriod
  /// Its lines, newest first.
  package var runs: [ActivityRun]

  /// The group's identity: its stretch of time, which no other group shares.
  package var id: ActivityPeriod { period }

  /// Creates a group.
  ///
  /// - Parameters:
  ///   - period: The stretch of time.
  ///   - runs: Its lines, newest first.
  package init(period: ActivityPeriod, runs: [ActivityRun]) {
    self.period = period
    self.runs = runs
  }
}
