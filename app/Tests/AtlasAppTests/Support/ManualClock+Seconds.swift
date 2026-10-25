import AsyncSupportTesting

extension ManualClock {
  /// The time the clock has advanced, in seconds: what the viewer reads as its uptime when a test hands it the clock,
  /// so a picture ages exactly as the test moves time.
  var seconds: Double {
    now.offset / .seconds(1)
  }
}
