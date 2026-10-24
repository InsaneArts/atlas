import Foundation

/// The outcome of one `atlas` command: whether it worked, its JSON reply,
/// and what to tell the person in one line.
public struct Outcome: Sendable {
  /// Whether the command worked.
  public let ok: Bool
  /// The reply as the command printed it (JSON), for decoding.
  public let reply: Data
  /// What to tell the person, in one line.
  public let message: String
  /// The failure's kind (`logout-held`, say), when it names one.
  public let code: String?

  /// Creates an outcome.
  ///
  /// - Parameters:
  ///   - ok: Whether the command worked.
  ///   - reply: The reply as the command printed it.
  ///   - message: What to tell the person, in one line.
  ///   - code: The failure's kind, when it names one.
  public init(ok: Bool, reply: Data, message: String, code: String? = nil) {
    self.ok = ok
    self.reply = reply
    self.message = message
    self.code = code
  }

  /// Whether an app in the helper held a log-out up, so it may be forced.
  public var isLogOutHeld: Bool { code == "logout-held" }

  /// The reply decoded as `type`, if it is one.
  ///
  /// - Parameter type: The reply's expected shape.
  /// - Returns: The decoded reply, or `nil` when the reply is not one.
  public func decode<T: Decodable>(_ type: T.Type) -> T? {
    let decoder = JSONDecoder()
    return try? decoder.decode(type, from: reply)
  }

  /// Read a command's output: replies go to stdout, errors to stderr.
  ///
  /// - Parameters:
  ///   - status: The command's exit status.
  ///   - stdout: What it printed on standard output.
  ///   - stderr: What it printed on standard error.
  /// - Returns: The outcome; a command that printed no JSON failed with its standard error as the message.
  public static func parse(status: Int32, stdout: Data, stderr: Data) -> Outcome {
    struct Reply: Decodable {
      let ok: Bool?
      let error: String?
      let code: String?
    }
    let decoder = JSONDecoder()
    for data in [stdout, stderr] where !data.isEmpty {
      if let reply = try? decoder.decode(Reply.self, from: data) {
        let worked = status == 0 && reply.ok != false
        return Outcome(
          ok: worked,
          reply: data,
          message: worked ? "Done" : (reply.error ?? "It did not work"),
          code: worked ? nil : reply.code
        )
      }
    }
    let text = String(decoding: stderr, as: UTF8.self)
      .trimmingCharacters(in: .whitespacesAndNewlines)
    return Outcome(ok: false, reply: Data(), message: text.isEmpty ? "Atlas did not answer" : text)
  }
}
