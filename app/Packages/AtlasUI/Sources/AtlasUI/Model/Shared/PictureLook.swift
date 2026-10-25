/// How a picture of the helper's screen is drawn.
public enum PictureLook: Sendable {
  /// As it is.
  case live
  /// Greyed and darkened: the helper needs the person, or an alert is over it.
  case dimmed
  /// Faded, so it reads as not live.
  case paused
  /// As it is, inside the blue rim of control.
  case inControl
}
