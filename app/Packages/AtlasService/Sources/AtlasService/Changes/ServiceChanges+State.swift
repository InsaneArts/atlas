import Foundation

extension ServiceChanges {
  /// The changes counted per kind, whether the feed is followed, and the readers waiting.
  struct State {
    /// The changes counted per kind.
    var counts: [ChangeKind: UInt64] = [:]
    /// Whether the feed is followed now.
    var followed = false
    /// The readers waiting, by their wait, with the kinds each shows.
    var waiting: [UUID: (kinds: Set<ChangeKind>, resume: CheckedContinuation<Void, Never>)] = [:]
    /// Waits cancelled before they began waiting.
    var cancelled: Set<UUID> = []

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
      let woken = waiting.filter { !$0.value.kinds.isDisjoint(with: kinds) }
      for id in woken.keys { waiting[id] = nil }
      return woken.values.map(\.resume)
    }

    /// Every reader waiting, no longer waiting.
    ///
    /// - Returns: The readers to wake.
    mutating func wakeAll() -> [CheckedContinuation<Void, Never>] {
      defer { waiting = [:] }
      return waiting.values.map(\.resume)
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
      if cancelled.remove(id) != nil || count(of: kinds) != seen { return true }
      waiting[id] = (kinds, resume)
      return false
    }

    /// The wait `id` is cancelled.
    ///
    /// - Parameter id: The wait.
    /// - Returns: The reader to wake, when it waits already.
    mutating func cancel(_ id: UUID) -> CheckedContinuation<Void, Never>? {
      guard let waiting = waiting.removeValue(forKey: id) else {
        cancelled.insert(id)
        return nil
      }
      return waiting.resume
    }
  }
}
