import Foundation

extension URLSession {
  /// A session the scripted protocol answers, every request under `secret` getting `answer`.
  ///
  /// - Parameters:
  ///   - secret: The capability URL's secret, which no other test uses.
  ///   - answer: What every request under it gets.
  /// - Returns: The session.
  static func scripted(secret: String, answer: ScriptedURLProtocol.Answer) -> URLSession {
    ScriptedURLProtocol.session(secret: secret, answer: answer)
  }
}
