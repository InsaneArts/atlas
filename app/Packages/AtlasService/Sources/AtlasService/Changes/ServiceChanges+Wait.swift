import Foundation

extension ServiceChanges {
  /// Where one reader's wait stands, from the moment it begins until it is woken.
  enum Wait {
    /// Begun, not waiting yet.
    case begun
    /// Cancelled before it waited: it ends as soon as it would wait.
    case cancelled
    /// Waiting for a change of `kinds`, woken by `resume`.
    case waiting(kinds: Set<ChangeKind>, resume: CheckedContinuation<Void, Never>)
  }
}
