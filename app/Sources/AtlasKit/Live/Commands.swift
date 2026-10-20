import Foundation

/// Runs the `atlas` command line. The app holds no policy of its own: every
/// action is a command, so the command line stays complete.
package struct Commands: CommandRunning {
  /// The `atlas` executable it runs.
  package let executable: URL
  /// The environment every command runs in: the app's own, with `ATLAS_ACTOR=viewer`, so what the person does from
  /// the app is recorded as theirs, not an agent's. Its settings arrive as `ATLAS_CONFIG`, which every command
  /// inherits.
  package let environment: [String: String]

  /// Creates a runner for one `atlas`.
  ///
  /// The app chooses which `atlas`, and hands its environment on; this type never reads either itself.
  ///
  /// - Parameters:
  ///   - executable: The `atlas` executable to run.
  ///   - environment: The app's environment, which every command inherits.
  package init(executable: URL, environment: [String: String]) {
    self.executable = executable
    self.environment = environment.merging(["ATLAS_ACTOR": "viewer"]) { $1 }
  }

  /// The `atlas` beside this app's own executable (in Atlas.app).
  ///
  /// - Parameters:
  ///   - executable: This app's executable.
  ///   - environment: The app's environment, which every command inherits.
  /// - Returns: A runner for the `atlas` in the same folder.
  package static func beside(_ executable: URL, environment: [String: String]) -> Commands {
    let folder = executable.deletingLastPathComponent()
    return Commands(executable: folder.appendingPathComponent("atlas"), environment: environment)
  }

  /// Run `atlas --json arguments…` without blocking the caller. `input`,
  /// if any, is written to the command's standard input: a secret travels
  /// this way, never in its arguments. Cancelling the calling task stops
  /// the command.
  ///
  /// - Parameters:
  ///   - arguments: The command's arguments, after `--json`.
  ///   - input: What to write to the command's standard input, if anything.
  /// - Returns: The command's outcome; one that could not start, or was cancelled first, failed.
  package func run(_ arguments: [String], input: Data?) async -> Outcome {
    let executable = executable
    let environment = environment
    let running = RunningProcess()
    return await withTaskCancellationHandler {
      await withCheckedContinuation { continuation in
        let queue = DispatchQueue.global(qos: .userInitiated)
        queue.async {
          continuation.resume(returning: Self.runToEnd(executable, environment, arguments, input, running))
        }
      }
    } onCancel: {
      running.stop()
    }
  }

  /// Runs `atlas --json` with an action's arguments to its end on the calling thread, blocking it, in the same
  /// environment as ``run(_:input:)``: for a launch with nothing else to do meanwhile, such as the notice that runs
  /// before any app does.
  ///
  /// - Parameter action: The command to run.
  /// - Returns: The command's outcome; one that could not start failed.
  package func runNow(_ action: Action) -> Outcome {
    Self.runToEnd(executable, environment, action.arguments, nil, RunningProcess())
  }

  /// Runs one command to its end on the calling thread: a background one for ``run(_:input:)``, the launch's own for
  /// ``runNow(_:)``.
  ///
  /// - Parameters:
  ///   - executable: The `atlas` executable to run.
  ///   - environment: The environment it runs in.
  ///   - arguments: Its arguments, after `--json`.
  ///   - input: What to write to its standard input, if anything.
  ///   - running: Stops it when the caller is cancelled.
  /// - Returns: The command's outcome.
  private static func runToEnd(
    _ executable: URL,
    _ environment: [String: String],
    _ arguments: [String],
    _ input: Data?,
    _ running: RunningProcess
  )
    -> Outcome
  {
    let process = Process()
    process.executableURL = executable
    process.arguments = ["--json"] + arguments
    process.environment = environment
    let stdout = Pipe()
    let stderr = Pipe()
    process.standardOutput = stdout
    process.standardError = stderr
    let stdin = input.map { _ in Pipe() }
    process.standardInput = stdin ?? FileHandle.nullDevice
    do {
      guard try running.start(process) else {
        return Outcome(ok: false, reply: Data(), message: "Cancelled")
      }
    } catch {
      return Outcome(ok: false, reply: Data(), message: error.localizedDescription)
    }
    if let stdin, let input {
      // A command that exits early closes the pipe: the write fails
      // with an error, caught here, instead of raising SIGPIPE.
      _ = fcntl(stdin.fileHandleForWriting.fileDescriptor, F_SETNOSIGPIPE, 1)
      try? stdin.fileHandleForWriting.write(contentsOf: input)
      try? stdin.fileHandleForWriting.close()
    }
    // Both outputs are read at once, so neither can fill and stall the other.
    let errors = CollectedOutput()
    let reading = DispatchGroup()
    reading.enter()
    let queue = DispatchQueue.global(qos: .userInitiated)
    queue.async {
      errors.data = stderr.fileHandleForReading.readDataToEndOfFile()
      reading.leave()
    }
    let out = stdout.fileHandleForReading.readDataToEndOfFile()
    reading.wait()
    process.waitUntilExit()
    return Outcome.parse(status: process.terminationStatus, stdout: out, stderr: errors.data)
  }
}
