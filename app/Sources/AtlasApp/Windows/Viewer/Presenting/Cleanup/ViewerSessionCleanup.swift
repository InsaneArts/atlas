import AtlasUI

/// Cleaning up one session's work, as its row and the Session menu offer it: Clean Up This Session's Work…, which
/// asks first, whether it can be chosen now, and once a cleanup ran, what each of its steps did.
struct ViewerSessionCleanup: Equatable {
  /// The session's key, which the command names.
  var session: String
  /// Where the cleanup stands.
  var stage: Stage
  /// Whether another session's cleanup is read or runs now, or may still run, so this one waits its turn, its row
  /// showing no spinner of its own.
  var waitsForAnother: Bool

  /// Creates the cleanup's state.
  ///
  /// - Parameters:
  ///   - session: The session's key.
  ///   - stage: Where the cleanup stands; ready by default.
  ///   - waitsForAnother: Whether another session's cleanup holds it; not by default.
  init(session: String, stage: Stage = .ready, waitsForAnother: Bool = false) {
    self.session = session
    self.stage = stage
    self.waitsForAnother = waitsForAnother
  }

  /// Whether this session's cleanup is read or runs now, which its row shows with a spinner.
  var isWorking: Bool { stage == .working }

  /// Whether Clean Up can be chosen: not while any cleanup is read or runs.
  var isEnabled: Bool { !isWorking && !waitsForAnother }

  /// What each step of the cleanup that ran did, in order; none before one ran, or while one runs.
  var receipt: [Fact] {
    if case .finished(let steps) = stage { return steps }
    return []
  }
}
