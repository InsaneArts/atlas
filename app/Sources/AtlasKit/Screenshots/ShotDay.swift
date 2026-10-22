/// The screenshots the Screenshots tab lists under one stretch of time, newest first.
package struct ShotDay: Equatable, Sendable, Identifiable {
  /// The stretch of time.
  package var period: ShotPeriod
  /// Its screenshots, newest first.
  package var shots: [Screenshot]

  /// The group's identity: its stretch of time, which no other group shares.
  package var id: ShotPeriod { period }

  /// Creates a group.
  ///
  /// - Parameters:
  ///   - period: The stretch of time.
  ///   - shots: Its screenshots, newest first.
  package init(period: ShotPeriod, shots: [Screenshot]) {
    self.period = period
    self.shots = shots
  }
}
