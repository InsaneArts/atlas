/// Where an alert's command stands, shown in place: asked, running inside its button, refused with why, or
/// done just before the alert closes.
public enum AlertCardPhase: Hashable, Sendable {
  /// Asked, waiting for an answer.
  case asking
  /// Its command runs: the button that runs it holds a spinner beside its own word, VoiceOver hears this word, such as
  /// "Recovering…", and every other button waits.
  case running(label: String)
  /// Its command was refused: why, in place of the message, and the buttons come back to try again.
  case refused(message: String)
  /// Its command worked: this title, such as "The helper is ready", this message in the message's place,
  /// such as "Mouse and keyboard work again.", and the buttons giving way to one, this word after a check, such as
  /// "Done", which Return presses to close it before it closes itself.
  case done(title: String, message: String, button: String)
}
