import Foundation
import os

/// A private URLSession script: its first release is refused and its retry waits until URLSession cancels it.
/// Each session's capability path owns its counters, so parallel tests reach neither another script nor the network.
nonisolated final class RetriedReleaseProtocol: URLProtocol {
  /// Request and cancellation observations, with how many requests each session has received.
  private static let scripts = OSAllocatedUnfairLock(
    initialState: [String: (requests: CallCount, cancellations: CallCount, received: Int)]()
  )
  /// Whether this protocol instance is holding the retry rather than answering the first request.
  private let held = OSAllocatedUnfairLock(initialState: false)

  /// Makes a session whose only answers come from this protocol.
  ///
  /// - Parameters:
  ///   - secret: The unique first path component used by the test's client.
  ///   - requests: Counts requests reaching the script.
  ///   - cancellations: Counts requests URLSession stops.
  /// - Returns: The isolated session.
  static func session(secret: String, requests: CallCount, cancellations: CallCount) -> URLSession {
    scripts.withLock { $0[secret] = (requests, cancellations, 0) }
    let configuration = URLSessionConfiguration.ephemeral
    configuration.protocolClasses = [RetriedReleaseProtocol.self]
    return URLSession(configuration: configuration)
  }

  /// Removes a finished test's script after its session has been invalidated.
  ///
  /// - Parameter secret: The session's unique path component.
  static func forget(_ secret: String) {
    scripts.withLock { $0.removeValue(forKey: secret) }
  }

  /// Handles every request in this private session.
  ///
  /// - Parameter request: The request.
  /// - Returns: True.
  override class func canInit(with request: URLRequest) -> Bool { true }

  /// Keeps the request unchanged.
  ///
  /// - Parameter request: The request.
  /// - Returns: The request.
  override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

  /// Refuses the first request; later ones stay unanswered until cancelled.
  override func startLoading() {
    let secret = request.url?.pathComponents.dropFirst().first ?? ""
    let script = Self.scripts
      .withLock { scripts in
        guard var script = scripts[secret] else { return scripts[secret] }
        script.received += 1
        scripts[secret] = script
        return script
      }
    guard let script else {
      client?.urlProtocol(self, didFailWithError: URLError(.badURL))
      return
    }
    held.withLock { $0 = script.received > 1 }
    Task { @MainActor in script.requests.count += 1 }
    guard script.received == 1 else { return }
    guard let url = request.url,
      let response = HTTPURLResponse(url: url, statusCode: 409, httpVersion: nil, headerFields: [:])
    else { return }
    client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
    client?.urlProtocol(self, didLoad: Data(#"{"ok":false,"code":"busy","error":"Still held"}"#.utf8))
    client?.urlProtocolDidFinishLoading(self)
  }

  /// Records URLSession stopping a held request, as the coordinator's ceiling cancels its retry.
  override func stopLoading() {
    guard held.withLock({ $0 }) else { return }
    let secret = request.url?.pathComponents.dropFirst().first ?? ""
    if let script = Self.scripts.withLock({ $0[secret] }) {
      Task { @MainActor in script.cancellations.count += 1 }
    }
  }
}
