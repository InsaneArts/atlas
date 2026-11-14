import Foundation

extension ServiceChanges {
  /// The changes counted per kind, whether the feed is followed, and the readers' waits.
  struct State {
    /// The changes counted per kind.
    var counts: [ChangeKind: UInt64] = [:]
    /// Whether the feed is followed now.
    var followed = false
    /// Every wait from its beginning until it is woken or ends.
    var waits: [UUID: Wait] = [:]

    /// How many changes of `kinds` were counted.
    ///
    /// - Parameter kinds: The kinds.
    /// - Returns: The count.
    func count(of kinds: Set<ChangeKind>) -> UInt64 {
      kinds.reduce(0) { $0 + counts[$1, default: 0] }
    }

    /// Counts a change of each of `kinds`.
    ///
    /// - Parameter kinds: What changed.
    /// - Returns: The readers to wake: those that show any of them.
    mutating func tell(_ kinds: Set<ChangeKind>) -> [CheckedContinuation<Void, Never>] {
      for kind in kinds { counts[kind, default: 0] += 1 }
      return wake { !$0.isDisjoint(with: kinds) }
    }

    /// Every reader waiting, no longer waiting.
    ///
    /// - Returns: The readers to wake.
    mutating func wakeAll() -> [CheckedContinuation<Void, Never>] {
      wake { _ in true }
    }

    /// The wait `id` begins.
    ///
    /// - Parameter id: The wait.
    mutating func begin(_ id: UUID) {
      waits[id] = .begun
    }

    /// Waits as `id` for a change of `kinds` since `seen`, unless one came or the wait was cancelled already.
    ///
    /// - Parameters:
    ///   - id: The wait.
    ///   - kinds: The kinds.
    ///   - seen: The count noted.
    ///   - resume: What wakes the reader.
    /// - Returns: Whether to wake it now instead.
    mutating func wait(
      _ id: UUID,
      for kinds: Set<ChangeKind>,
      since seen: UInt64,
      _ resume: CheckedContinuation<Void, Never>
    ) -> Bool {
      guard case .begun = waits[id], count(of: kinds) == seen else {
        waits[id] = nil
        return true
      }
      waits[id] = .waiting(kinds: kinds, resume: resume)
      return false
    }

    /// The wait `id` is cancelled.
    ///
    /// - Parameter id: The wait.
    /// - Returns: The reader to wake, when it waits already.
    mutating func cancel(_ id: UUID) -> CheckedContinuation<Void, Never>? {
      switch waits[id] {
      case .begun:
        waits[id] = .cancelled
        return nil
      case .waiting(_, let resume):
        waits[id] = nil
        return resume
      case .cancelled, nil:
        return nil
      }
    }

    /// The wait `id` ended: nothing of it is kept.
    ///
    /// - Parameter id: The wait.
    mutating func end(_ id: UUID) {
      waits[id] = nil
    }

    /// The readers waiting for kinds `wanted` picks, no longer waiting.
    ///
    /// - Parameter wanted: Whether a reader of those kinds is woken.
    /// - Returns: The readers to wake.
    private mutating func wake(_ wanted: (Set<ChangeKind>) -> Bool) -> [CheckedContinuation<Void, Never>] {
      var woken: [CheckedContinuation<Void, Never>] = []
      for (id, wait) in waits {
        guard case .waiting(let kinds, let resume) = wait, wanted(kinds) else { continue }
        waits[id] = nil
        woken.append(resume)
      }
      return woken
    }
  }
}
