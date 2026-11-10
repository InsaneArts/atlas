import Foundation

/// The lines the parent writes to the app's standard input, read on a thread of their own from the start, so the
/// launch can wait for one for a while and go on without it: the viewer's URL, this launch's secret, then the lines the
/// app follows. One reader, so no two threads ever read standard input.
nonisolated final class LaunchInput: Sendable {
  /// What waiting for a line came to.
  enum Next: Equatable, Sendable {
    /// The next line.
    ///
    /// - Parameter line: The line.
    case line(String)
    /// Standard input closed.
    case closed
    /// No line came in time; it may still come.
    case timedOut
  }

  /// The lines read and not yet taken, and whether input closed, under their lock.
  private let state = NSCondition()
  /// The lines read and not yet taken. Touched only under `state`.
  nonisolated(unsafe) private var lines: [String] = []
  /// Whether input closed. Touched only under `state`.
  nonisolated(unsafe) private var ended = false

  /// Starts reading lines with `read`, which returns nil once input closes.
  ///
  /// - Parameter read: Reads one line, blocking; standard input's by default.
  init(read: @escaping @Sendable () -> String? = { readLine() }) {
    Thread.detachNewThread { [self] in
      while let line = read() {
        state.lock()
        lines.append(line)
        state.signal()
        state.unlock()
      }
      state.lock()
      ended = true
      state.broadcast()
      state.unlock()
    }
  }

  /// The next line, waiting at most `seconds` for it; forever when nil.
  ///
  /// - Parameter seconds: The longest wait, or nil for none.
  /// - Returns: The line, or that input closed or no line came in time.
  func next(within seconds: TimeInterval? = nil) -> Next {
    let deadline = seconds.map { Date(timeIntervalSinceNow: $0) }
    state.lock()
    defer { state.unlock() }
    while lines.isEmpty && !ended {
      if let deadline {
        guard state.wait(until: deadline) else { return lines.isEmpty && !ended ? .timedOut : take() }
      } else {
        state.wait()
      }
    }
    return take()
  }

  /// Takes the oldest line, or says input closed. Called under `state`.
  ///
  /// - Returns: The line, or closed.
  private func take() -> Next {
    guard !lines.isEmpty else { return .closed }
    return .line(lines.removeFirst())
  }

  /// Passes each line on as it comes, on the main actor, and says once input closes.
  ///
  /// - Parameters:
  ///   - received: Called with each line.
  ///   - closed: Called once input closes.
  func listen(
    received: @escaping @MainActor @Sendable (String) -> Void,
    closed: @escaping @MainActor @Sendable () -> Void
  ) {
    Thread.detachNewThread { [self] in
      // Unstructured, as this thread is outside any actor: each line, then the close, hops to the main actor.
      while case .line(let line) = next() {
        Task { @MainActor in received(line) }
      }
      Task { @MainActor in closed() }
    }
  }
}
