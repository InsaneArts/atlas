import Foundation
import os

/// Answers a test's requests from a script, never the network: each test registers what its own capability URL's
/// requests get, by the secret in their path, so tests running at once never see one another's replies.
final class ScriptedURLProtocol: URLProtocol {
  /// What a request gets.
  enum Answer: Sendable {
    /// A reply with this status and body.
    case reply(status: Int, body: Data)
    /// A failure, such as a timeout or a cancellation.
    case failure(URLError.Code)
  }

  /// Each test's answer, by its capability URL's secret. URLProtocol makes its own instances, so the script lives
  /// with the class; the lock keeps tests running at once apart.
  private static let script = OSAllocatedUnfairLock(initialState: [String: Answer]())

  /// A session whose requests this protocol answers, and the secret its answers go under.
  ///
  /// - Parameters:
  ///   - secret: The capability URL's secret, which no other test uses.
  ///   - answer: What every request under it gets.
  /// - Returns: The session.
  static func session(secret: String, answer: Answer) -> URLSession {
    script.withLock { $0[secret] = answer }
    let configuration = URLSessionConfiguration.ephemeral
    configuration.protocolClasses = [ScriptedURLProtocol.self]
    return URLSession(configuration: configuration)
  }

  /// Takes every request: the session is a test's alone.
  ///
  /// - Parameter request: The request.
  /// - Returns: True.
  override class func canInit(with request: URLRequest) -> Bool { true }

  /// The request as it is.
  ///
  /// - Parameter request: The request.
  /// - Returns: The same request.
  override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

  /// Answers from the script, by the first part of the request's path.
  override func startLoading() {
    let secret = request.url?.pathComponents.dropFirst().first ?? ""
    let scripted = Self.script
      .withLock { $0[secret] }
    let answer = scripted ?? .failure(.cannotConnectToHost)
    switch answer {
    case .reply(let status, let body):
      guard
        let url = request.url,
        let response = HTTPURLResponse(url: url, statusCode: status, httpVersion: nil, headerFields: nil)
      else { return client?.urlProtocol(self, didFailWithError: URLError(.badURL)) ?? () }
      client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
      client?.urlProtocol(self, didLoad: body)
      client?.urlProtocolDidFinishLoading(self)
    case .failure(let code):
      client?.urlProtocol(self, didFailWithError: URLError(code))
    }
  }

  /// Nothing to stop: every answer is given at once.
  override func stopLoading() {}
}
