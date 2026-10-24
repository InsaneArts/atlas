import AtlasKit

/// A Recover as it ran: which recovery the helper's state called for, and how it went.
struct HelperRecovery {
  /// The recovery run: a restart, or starting the helper's input.
  let action: Action
  /// How it went, a refused release included.
  let outcome: Outcome

  /// What a window says once it ran: the helper recovered or ready again, or why not in plain words.
  var notice: String {
    guard outcome.ok else { return HelperWords.recoverRefused(outcome) }
    return action == .restart ? Words.recovered : Words.readyAgain
  }

  /// What a window says when the viewer service's own Recover (``ViewerService/recover()``) failed: that it still
  /// runs, when it gave no answer in time and carries on, else the service's words. Every caller of `recover()` words
  /// its errors here, since the late one's message is generic.
  ///
  /// - Parameter error: Why the service's Recover failed.
  /// - Returns: "The helper is still recovering. This can take a minute.", or the service's error as a sentence.
  static func notice(_ error: ServiceError) -> String {
    error.isStillRecovering ? Words.stillRecovering : Words.sentence(error.message)
  }
}
