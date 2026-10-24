import Foundation

/// A command that may be stopped from another task: started at most
/// once, and never after it was stopped.
///
/// `@unchecked Sendable` because the lock guards every access to its state:
/// the command's own thread starts it and the cancelling task stops it.
final class RunningProcess: @unchecked Sendable {
  private let lock = NSLock()
  private var process: Process?
  private var stopped = false

  /// Starts `process`, unless already stopped.
  ///
  /// - Parameter process: The command to start.
  /// - Returns: Whether it runs: false once stopped.
  /// - Throws: Why the command could not start.
  func start(_ process: Process) throws -> Bool {
    try lock.withLock {
      guard !stopped else { return false }
      try process.run()
      self.process = process
      return true
    }
  }

  /// Stops the command: it ends if it runs, and never starts if it has not yet.
  func stop() {
    lock.withLock {
      stopped = true
      if process?.isRunning == true { process?.terminate() }
    }
  }
}
