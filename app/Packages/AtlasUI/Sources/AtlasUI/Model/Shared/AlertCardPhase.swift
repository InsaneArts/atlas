/// Where an alert's command stands, shown in place (v3 A3–A5): asked, running inside its button, refused with why,
/// or done just before the alert closes.
public enum AlertCardPhase: Hashable, Sendable {
  /// Asked, waiting for an answer.
  case asking
  /// Its command runs: the button that runs it holds a spinner and this word, such as "Recovering…", and every
  /// other button waits.
  case running(label: String)
  /// Its command was refused: why, in place of the message, and the buttons come back to try again.
  case refused(message: String)
  /// Its command worked: a check in the button, and this title, such as "Recovered", until the alert closes.
  case done(title: String)
}
