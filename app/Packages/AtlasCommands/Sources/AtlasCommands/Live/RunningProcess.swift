import Foundation
import os

/// A command that may be stopped from another task: started at most
/// once, and never after it was stopped.
///
/// The command's own thread starts it and the cancelling task stops it, so
/// its state lives behind one lock. `OSAllocatedUnfairLock`, as `Mutex`
/// needs macOS 15.
final class RunningProcess: Sendable {
  /// The command once started, and whether it was stopped.
  private struct State {
    /// The command, once started.
    var process: Process?
    /// Whether it was stopped.
    var stopped = false
  }

  /// The state, read and written only under its lock.
  private let state = OSAllocatedUnfairLock(initialState: State())

  /// Starts `process`, unless already stopped.
  ///
  /// - Parameter process: The command to start.
  /// - Returns: Whether it runs: false once stopped.
  /// - Throws: Why the command could not start.
  func start(_ process: Process) throws -> Bool {
    try state.withLock { state in
      guard !state.stopped else { return false }
      try process.run()
      state.process = process
      return true
    }
  }

  /// Stops the command: it ends if it runs, and never starts if it has not yet.
  func stop() {
    state.withLock { state in
      state.stopped = true
      if state.process?.isRunning == true { state.process?.terminate() }
    }
  }
}
