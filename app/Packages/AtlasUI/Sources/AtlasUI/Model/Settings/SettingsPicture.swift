/// How often Atlas's windows show the helper's screen, as Settings offers it.
public enum SettingsPicture: Hashable, Sendable {
  /// As often as a picture is worth reading.
  case live
  /// A picture every five seconds.
  case everyFiveSeconds
  /// No new pictures in the main window.
  case paused
}
