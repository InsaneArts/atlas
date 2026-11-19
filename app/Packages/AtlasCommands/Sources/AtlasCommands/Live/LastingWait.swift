import AtlasService
import Foundation
import os

/// The wait for a lasting command, ended once: by the command's end, or by the caller giving up on it, whichever comes
/// first. Its end and the giving up come on other threads, so the waiting caller lives behind one lock.
final class LastingWait: Sendable {
  /// The caller waiting, until it is answered.
  private let waiting = OSAllocatedUnfairLock<CheckedContinuation<Outcome, Never>?>(initialState: nil)

  /// What a wait given up on answers: the command still runs.
  static let givenUp = Outcome(ok: false, reply: Data(), message: "It still runs", code: "still-running")

  /// Holds `continuation` until ``end(_:)``; answers it at once when the wait was given up on already.
  ///
  /// - Parameter continuation: The caller waiting.
  func hold(_ continuation: CheckedContinuation<Outcome, Never>) {
    waiting.withLock { $0 = continuation }
  }

  /// Answers the caller with `outcome`, once: a second end changes nothing.
  ///
  /// - Parameter outcome: What the wait came to.
  func end(_ outcome: Outcome) {
    let caller = waiting.withLock { waiting in
      defer { waiting = nil }
      return waiting
    }
    caller?.resume(returning: outcome)
  }
}
