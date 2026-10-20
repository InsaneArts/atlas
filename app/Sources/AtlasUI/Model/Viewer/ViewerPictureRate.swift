/// How often the viewer reads a picture while watching, in seconds. The raw values are saved as the person's choice
/// and never change.
package enum ViewerPictureRate: Double, CaseIterable, Sendable {
  /// Every second.
  case everySecond = 1
  /// Every 2 seconds, the default.
  case everyTwoSeconds = 2
  /// Every 5 seconds.
  case everyFiveSeconds = 5

  /// Seconds between pictures.
  package var seconds: Double { rawValue }
}
