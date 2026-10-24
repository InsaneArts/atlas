import os

/// A clock a test moves by hand: `sleep` waits until ``advance(by:)`` reaches its deadline, so a test of an interval,
/// a heartbeat or a pause runs at once and never sleeps on the wall clock. The app's controllers take it where the app
/// passes `ContinuousClock()`.
///
/// A test learns that something sleeps from ``sleepers()`` (or ``untilSleeping(_:for:_:within:sourceLocation:)``),
/// never by yielding or polling: it waits for the sleeper it expects, then advances. ``advance(by:)`` wakes every
/// sleeper it reaches in deadline order, the earliest first and two with one deadline in the order they began. A
/// cancelled sleeper throws `CancellationError` and is forgotten at once, whether it was cancelled while waiting or
/// before it began to wait, so the clock holds nothing for a sleeper that has returned.
///
/// It is a class guarded by a lock rather than an actor because a sleeper's cancellation handler runs synchronously on
/// any thread and must remove the sleeper at once. Each change records the deadlines left waiting under the lock, and
/// they are handed to readers only outside it, under a second lock no handler takes, in the order the changes
/// happened. This is jolt's `ManualClock`, on `OSAllocatedUnfairLock` since Atlas runs on macOS 14.
public final class ManualClock: Clock, Sendable {
  /// A point on the manual clock: the time advanced since the clock was made.
  public struct Instant: InstantProtocol {
    /// The time since the clock started.
    public var offset: Duration

    /// Creates an instant.
    ///
    /// - Parameter offset: The time since the clock started.
    public init(offset: Duration) {
      self.offset = offset
    }

    /// The instant `duration` after this one.
    ///
    /// - Parameter duration: How far to move.
    /// - Returns: The later instant.
    public func advanced(by duration: Duration) -> Instant {
      Instant(offset: offset + duration)
    }

    /// The time from this instant to `other`.
    ///
    /// - Parameter other: The later instant.
    /// - Returns: The duration between them.
    public func duration(to other: Instant) -> Duration {
      other.offset - offset
    }

    /// Whether `lhs` comes before `rhs`.
    ///
    /// - Parameters:
    ///   - lhs: One instant.
    ///   - rhs: Another.
    /// - Returns: Whether `lhs` is earlier.
    public static func < (lhs: Instant, rhs: Instant) -> Bool {
      lhs.offset < rhs.offset
    }
  }

  /// What a sleeper resumes with: woken, or cancelled.
  private typealias Wake = CheckedContinuation<Result<Void, CancellationError>, Never>

  /// One call to `sleep`, from the moment it takes an id until it returns.
  private enum Sleeper: Sendable {
    /// The call has its id but has not yet checked its deadline.
    case starting

    /// The call is waiting for the clock to reach its deadline.
    case waiting(deadline: Instant, wake: Wake)

    /// The task was cancelled before the call began to wait; the call throws as soon as it looks.
    case cancelledEarly
  }

  /// One reader of ``deadlines()``.
  private struct Observer: Sendable {
    /// Hands the reader the deadlines left waiting.
    let yield: @Sendable ([Instant]) -> Void

    /// Ends the reader's stream.
    let finish: @Sendable () -> Void
  }

  /// The clock's state: now, the sleepers by id, and the readers of ``deadlines()``.
  private struct State: Sendable {
    /// Now.
    var now = Instant(offset: .zero)

    /// Every call to `sleep` that has not yet returned, by id; an entry is removed as its call returns or throws. Ids
    /// rise, so a lower id began first.
    var sleepers: [Int: Sleeper] = [:]

    /// The id the next call to `sleep` takes.
    var nextSleeper = 0

    /// The readers, by id.
    var observers: [Int: Observer] = [:]

    /// The id the next reader takes.
    var nextObserver = 0

    /// The id of every sleeper woken so far, in the order it was resumed.
    var woken: [Int] = []

    /// Deadlines recorded at their changes and not yet handed out, in order, each with the readers live at the time.
    var outbox: [(observers: [Observer], deadlines: [Instant])] = []

    /// The deadline of every sleeper waiting, soonest first.
    var deadlines: [Instant] {
      sleepers.values
        .compactMap { sleeper in
          if case .waiting(let deadline, _) = sleeper {
            return deadline
          }
          return nil
        }
        .sorted()
    }

    /// Records the deadlines waiting now for every reader, to be handed out after the lock is released.
    mutating func record() {
      outbox.append((Array(observers.values), deadlines))
    }
  }

  /// The clock's state, guarded because sleepers, their cancellation handlers and the test touch it from any thread.
  private let state = OSAllocatedUnfairLock(initialState: State())

  /// Held while recorded deadlines are handed to readers, so they arrive in the order the changes happened; no
  /// cancellation or termination handler takes it.
  private let delivery = OSAllocatedUnfairLock()

  /// Creates a clock at its start.
  public init() {}

  /// Finishes every stream from ``deadlines()`` and ``sleepers()``, so no reader waits for a clock that is gone.
  ///
  /// A sleeping call holds the clock, so no sleeper is left waiting when it goes away.
  deinit {
    let observers = state.withLock { state in
      defer { state.observers.removeAll() }
      return Array(state.observers.values)
    }
    // Finished outside the lock: finishing runs a stream's termination handler, which takes the lock itself.
    for observer in observers {
      observer.finish()
    }
  }

  /// Now.
  public var now: Instant {
    state.withLock { $0.now }
  }

  /// The smallest step: none, since the clock moves only when told.
  public var minimumResolution: Duration {
    .zero
  }

  /// How long each sleeper waiting now has left, soonest first: what a test reads to see which pauses were asked for.
  public var waiting: [Duration] {
    state.withLock { state in
      state.deadlines.map { state.now.duration(to: $0) }
    }
  }

  /// How many calls to `sleep` the clock still holds, waiting or not yet waiting; for tests of the clock itself.
  var registered: Int {
    state.withLock { $0.sleepers.count }
  }

  /// How many streams from ``deadlines()`` and ``sleepers()`` are live; for tests of the clock itself.
  var observing: Int {
    state.withLock { $0.observers.count }
  }

  /// The id of every sleeper woken so far, in the order it was resumed (ids rise in the order sleepers began); for
  /// tests of the clock itself, since the order woken sleepers then run in is their executor's.
  var woken: [Int] {
    state.withLock { $0.woken }
  }

  /// Waits until the clock reaches `deadline`, or throws `CancellationError` when the task is cancelled, before or
  /// while it waits. A deadline the clock has already reached returns at once.
  ///
  /// ``advance(by:)`` wakes sleepers in deadline order, but the order they then run in is their executor's (a call
  /// through `any Clock<Duration>` resumes off its actor and hops back), so a test that needs one woken sleeper's work
  /// done before another's advances to each deadline in turn.
  ///
  /// - Parameters:
  ///   - deadline: When to wake.
  ///   - tolerance: Ignored: a sleeper wakes exactly when the clock reaches its deadline.
  /// - Throws: `CancellationError` when the task is cancelled.
  nonisolated(nonsending) public func sleep(until deadline: Instant, tolerance: Duration?)
    async throws(CancellationError)
  {
    let id = state.withLock { state in
      let id = state.nextSleeper
      state.nextSleeper += 1
      state.sleepers[id] = .starting
      return id
    }
    // The continuation never throws: it carries the outcome, so the typed error survives the resume.
    var waited = false
    let result = await withTaskCancellationHandler {
      await withCheckedContinuation { (wake: Wake) in
        let outcome: Result<Void, CancellationError>? = change { state in
          if case .cancelledEarly = state.sleepers[id] {
            state.sleepers[id] = nil
            return (.failure(CancellationError()), publishes: false)
          }
          if deadline <= state.now {
            state.sleepers[id] = nil
            return (.success(()), publishes: false)
          }
          state.sleepers[id] = .waiting(deadline: deadline, wake: wake)
          return (nil, publishes: true)
        }
        if let outcome {
          wake.resume(returning: outcome)
        } else {
          waited = true
        }
      }
    } onCancel: {
      self.cancel(id)
    }
    // The cancellation handler recorded the deadlines this sleeper left; hand them out here, in its own task.
    if waited, case .failure = result {
      handOut()
    }
    try result.get()
  }

  /// Moves the clock forward and wakes every sleeper whose deadline it reaches, the earliest deadline first, then
  /// publishes the deadlines left waiting, even when none changed.
  ///
  /// - Parameter duration: How far to move.
  public func advance(by duration: Duration) {
    let due = change { state in
      state.now = state.now.advanced(by: duration)
      var due: [(deadline: Instant, id: Int, wake: Wake)] = []
      for (id, sleeper) in state.sleepers {
        if case .waiting(let deadline, let wake) = sleeper, deadline <= state.now {
          state.sleepers[id] = nil
          due.append((deadline, id, wake))
        }
      }
      // Deadline order, then the order the sleepers began: a dictionary keeps neither.
      due.sort { ($0.deadline, $0.id) < ($1.deadline, $1.id) }
      state.woken += due.map(\.id)
      return (due.map(\.wake), publishes: true)
    }
    for wake in due {
      wake.resume(returning: .success(()))
    }
  }

  /// The deadlines of the sleepers waiting, soonest first, now and after each change, for a test that waits until
  /// something sleeps.
  ///
  /// The stream starts with the deadlines waiting now, then gives them again after each sleeper starts or stops
  /// waiting and after each ``advance(by:)``, in order. Each call returns a stream of its own.
  ///
  /// - Returns: A stream that ends when its reader stops or the clock goes away.
  public func deadlines() -> AsyncStream<[Instant]> {
    let (stream, continuation) = AsyncStream.makeStream(of: [Instant].self)
    let observer = Observer(yield: { continuation.yield($0) }, finish: { continuation.finish() })
    observe(observer) { id in
      continuation.onTermination = { [weak self] _ in self?.forget(id) }
    }
    return stream
  }

  /// The number of sleepers waiting, now and after each change, as ``deadlines()`` gives their deadlines.
  ///
  /// - Returns: A stream that ends when its reader stops or the clock goes away.
  public func sleepers() -> AsyncStream<Int> {
    let (stream, continuation) = AsyncStream.makeStream(of: Int.self)
    let observer = Observer(yield: { continuation.yield($0.count) }, finish: { continuation.finish() })
    observe(observer) { id in
      continuation.onTermination = { [weak self] _ in self?.forget(id) }
    }
    return stream
  }

  /// Adds a reader, hands it the deadlines waiting now, and lets its stream forget it once it stops.
  ///
  /// - Parameters:
  ///   - observer: The reader.
  ///   - ends: Sets the stream's termination handler, given the reader's id.
  private func observe(_ observer: Observer, ends: (Int) -> Void) {
    let id = state.withLock { state in
      let id = state.nextObserver
      state.nextObserver += 1
      state.observers[id] = observer
      state.outbox.append(([observer], state.deadlines))
      return id
    }
    ends(id)
    handOut()
  }

  /// Forgets a reader whose stream ended.
  ///
  /// - Parameter id: The reader's id.
  private func forget(_ id: Int) {
    state.withLock { _ = $0.observers.removeValue(forKey: id) }
  }

  /// Changes the state, records the deadlines right after it when the change says readers hear of it, and hands out
  /// every recorded change, so readers see one list per change, in the order the changes happened.
  ///
  /// - Parameter body: The change; it returns its result and whether readers hear of it.
  /// - Returns: The change's result.
  private func change<Result: Sendable>(_ body: @Sendable (inout State) -> (Result, publishes: Bool)) -> Result {
    let result = state.withLock { state in
      let (result, publishes) = body(&state)
      if publishes {
        state.record()
      }
      return result
    }
    handOut()
    return result
  }

  /// Hands out every list recorded so far, in order, outside the state lock; never called from a handler.
  private func handOut() {
    delivery.withLock {
      let recorded = state.withLock { state in
        defer { state.outbox.removeAll() }
        return state.outbox
      }
      for (observers, deadlines) in recorded {
        for observer in observers {
          observer.yield(deadlines)
        }
      }
    }
  }

  /// Handles a sleeper's cancellation: a waiting sleeper is removed, its change recorded, and it throws, handing the
  /// change out from its own call once it is resumed; one that has not begun to wait is marked, so it throws as soon
  /// as it looks; one that has already returned is not in the table, so nothing is kept.
  ///
  /// - Parameter id: The sleeper's id.
  private func cancel(_ id: Int) {
    let wake = state.withLock { state -> Wake? in
      switch state.sleepers[id] {
      case .starting:
        state.sleepers[id] = .cancelledEarly
        return nil
      case .waiting(_, let wake):
        state.sleepers[id] = nil
        state.record()
        return wake
      case .cancelledEarly, nil:
        return nil
      }
    }
    wake?.resume(returning: .failure(CancellationError()))
  }
}
