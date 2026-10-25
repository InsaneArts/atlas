/// How often the main window reads a picture while watching, in seconds. The raw values are saved as the person's
/// choice and never change; a saved one no longer offered reads as ``live``.
enum ViewerPictureRate: Double, CaseIterable, Sendable {
  /// Every 2 seconds, the default: as often as a picture is worth reading.
  case live = 2
  /// Every 5 seconds.
  case everyFiveSeconds = 5

  /// Seconds between pictures.
  var seconds: Double { rawValue }
}
