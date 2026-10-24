import AtlasService
import Foundation

/// Something that runs `atlas` commands; the app's models take this, so
/// tests can stand in for the command line.
///
/// ``Commands`` is the live conformer; the app's tests script their own.
public protocol CommandRunning: Sendable {
  /// Runs `atlas --json` with `arguments`.
  ///
  /// - Parameters:
  ///   - arguments: The command's arguments, after `--json`.
  ///   - input: What to write to the command's standard input, if anything: a secret travels this way.
  /// - Returns: The command's outcome; a command that could not run is a failed one.
  func run(_ arguments: [String], input: Data?) async -> Outcome
}

extension CommandRunning {
  /// Runs `atlas --json` with `arguments` and nothing on its standard input.
  ///
  /// - Parameter arguments: The command's arguments, after `--json`.
  /// - Returns: The command's outcome.
  public func run(_ arguments: [String]) async -> Outcome {
    await run(arguments, input: nil)
  }

  /// Runs `atlas --json` with an action's arguments.
  ///
  /// - Parameters:
  ///   - action: The command to run.
  ///   - input: What to write to its standard input, if anything: a secret travels this way.
  /// - Returns: The command's outcome.
  public func run(_ action: Action, input: Data? = nil) async -> Outcome {
    await run(action.arguments, input: input)
  }
}
