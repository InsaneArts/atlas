import AtlasKit
import Foundation

/// A stand-in command line: answers by the first arguments, records what ran and what it was given, and can run a
/// step of the test while a command runs.
/// It is main-actor bound, as every test that uses it is, so its state is only ever touched there.
@MainActor
final class ScriptedCommands: CommandRunning {
  /// The reply to each command, by its first two arguments, or its first.
  private var answers: [String: String]
  /// What ran, in order, with what it was given on standard input.
  private var log: [([String], Data?)] = []
  /// Runs while a command runs, before it answers.
  private var step: (@MainActor ([String]) async -> Void)?

  /// A command line that answers each command with its reply.
  ///
  /// - Parameter answers: The JSON reply to each command, by its first two arguments, or its first.
  init(_ answers: [String: String]) {
    self.answers = answers
  }

  /// The arguments of each command run, in order.
  var ran: [[String]] { log.map(\.0) }
  /// What each command run was given on standard input, in order.
  var inputs: [Data?] { log.map(\.1) }

  /// Changes what `key` answers from now on.
  ///
  /// - Parameters:
  ///   - key: The command's first two arguments, or its first.
  ///   - reply: Its JSON reply.
  func answer(_ key: String, _ reply: String) {
    answers[key] = reply
  }

  /// Runs `step` while each command from now on runs, before it answers: a test does there what the person does
  /// while a command is under way.
  ///
  /// - Parameter step: What to run, given the command's arguments.
  func whileRunning(_ step: @escaping @MainActor ([String]) async -> Void) {
    self.step = step
  }

  /// Records the command, runs the step, then answers it, or fails it as unknown.
  ///
  /// - Parameters:
  ///   - arguments: The command's arguments.
  ///   - input: What it was given on standard input.
  /// - Returns: Its scripted outcome.
  func run(_ arguments: [String], input: Data?) async -> Outcome {
    log.append((arguments, input))
    await step?(arguments)
    let command = arguments.prefix(2)
      .joined(separator: " ")
    let reply = answers[command] ?? answers[arguments.first ?? ""]
    guard let reply else { return Outcome(ok: false, reply: Data(), message: "unknown command") }
    return Outcome.parse(status: 0, stdout: Data(reply.utf8), stderr: Data())
  }
}
