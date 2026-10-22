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
}
