/// The screenshots the Screenshots tab lists under one stretch of time, newest first.
public struct ShotDay: Equatable, Sendable, Identifiable {
  /// The stretch of time.
  public var period: ShotPeriod
  /// Its screenshots, newest first.
  public var shots: [Screenshot]

  /// The group's identity: its stretch of time, which no other group shares.
  public var id: ShotPeriod { period }

  /// Creates a group.
  ///
  /// - Parameters:
  ///   - period: The stretch of time.
  ///   - shots: Its screenshots, newest first.
  init(period: ShotPeriod, shots: [Screenshot]) {
    self.period = period
    self.shots = shots
  }
}
