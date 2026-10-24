import AtlasUI

/// Where an alert's command stands in a window's controller, before its words: asked, running, refused with why, or
/// done and holding for ``AtlasUI/Motion/Duration/doneHold`` before it closes. ``HelperAlert`` words it as the
/// alert's phase.
enum AlertProgress: Equatable {
  /// Asked, waiting for an answer.
  case asking
  /// Its command runs, with a note in place of the sentence, such as to try another alert again once it ends; nil for
  /// none.
  case running(note: String?)
  /// Its command was refused, saying why.
  case refused(String)
  /// Its command worked; the alert closes once the hold ends.
  case done

  /// The progress a controller's flags add up to: done over running, running over a refusal; a failure while its
  /// command runs is the running note.
  ///
  /// - Parameters:
  ///   - running: Whether its command runs now.
  ///   - failure: Why its command did not run, or nil for none.
  ///   - done: Whether its command worked and the alert holds before closing.
  /// - Returns: The progress.
  static func of(running: Bool, failure: String?, done: Bool = false) -> AlertProgress {
    if done { return .done }
    if running { return .running(note: failure) }
    return failure.map(AlertProgress.refused) ?? .asking
  }

  /// What the alert says in place of its sentence while its command runs; nil for its own sentence.
  var note: String? {
    if case .running(let note) = self { return note }
    return nil
  }

  /// The alert's phase, worded. Once done, one Done button closes it.
  ///
  /// - Parameters:
  ///   - running: What VoiceOver hears for the running button, such as "Recovering…".
  ///   - done: The title once done, such as "The helper is ready".
  ///   - doneMessage: The message once done, such as "Mouse and keyboard work again.".
  /// - Returns: The phase.
  func phase(running: String, done: String, doneMessage: String) -> AlertCardPhase {
    switch self {
    case .asking: .asking
    case .running: .running(label: running)
    case .refused(let message): .refused(message: message)
    case .done: .done(title: done, message: doneMessage, button: HelperWords.done)
    }
  }
}
