import Foundation
import Testing

@testable import AtlasService

@Suite("ViewerClient+Helper")
struct ViewerClientFetchTests {
  /// A client whose requests the scripted protocol answers.
  ///
  /// - Parameter answer: What every request gets.
  /// - Returns: The client.
  /// - Throws: When the capability URL is refused, which a fixed one never is.
  private func client(_ answer: ScriptedURLProtocol.Answer) throws -> ViewerClient {
    let secret = UUID().uuidString
    return try #require(
      ViewerClient("http://127.0.0.1:8765/\(secret)/", session: .scripted(secret: secret, answer: answer))
    )
  }

  @Test("frame headers accept only finite supported display sizes")
  func frameDisplaySizes() async throws {
    for side in ["1e100", "inf", "nan", "-1", "0", "32769", "32768"] {
      for dimension in ["X-Display-Width", "X-Display-Height"] {
        var headers = [
          "X-Frame-Id": "frame", "X-Session-Revision": "revision",
          "X-Display-Width": "1", "X-Display-Height": "1",
        ]
        headers[dimension] = side
        let reading = try client(.reply(status: 200, body: Data(), headers: headers))
        if side == "32768" {
          #expect(try await reading.frame().display.isValid)
        } else {
          await #expect(throws: ServiceError.unknownScreenSize) { try await reading.frame() }
        }
      }
    }
  }

  @Test("a reply at its size limit is read, and one a byte over is refused")
  func theSizeLimitHolds() async throws {
    let limit = 64
    let atLimit = try client(.reply(status: 200, body: Data(count: limit)))
    let request = atLimit.request(to: atLimit.base.appendingPathComponent("activity.json"))
    #expect(try await atLimit.fetched(request, limit: limit).0.count == limit)
    let over = try client(.reply(status: 200, body: Data(count: limit + 1)))
    await #expect(throws: ServiceError(ViewerClient.tooLarge)) {
      try await over.fetched(over.request(to: over.base.appendingPathComponent("activity.json")), limit: limit)
    }
  }

  @Test("a request that times out says the helper did not answer, and a cancelled one says it was cancelled")
  func timeoutsAndCancellationsReadApart() async throws {
    let late = try client(.failure(.timedOut))
    await #expect(throws: ServiceError(ViewerClient.lateReply)) { try await late.screenshots() }
    let cancelled = try client(.failure(.cancelled))
    await #expect(throws: ServiceError(ViewerClient.cancelledRequest, code: ViewerClient.cancelledCode)) {
      try await cancelled.screenshots()
    }
  }

  @Test("a recovery with no answer in time is reported as still recovering, a value with no words of its own")
  func aLateRecoveryIsAValue() async throws {
    let late = try client(.failure(.timedOut))
    let error = await #expect(throws: ServiceError.self) { try await late.recover() }
    #expect(error?.isStillRecovering == true)
    let other = try client(.failure(.timedOut))
    let screenshots = await #expect(throws: ServiceError.self) { try await other.screenshots() }
    #expect(screenshots?.isStillRecovering == false, "only Recover carries on in the service")
  }

  @Test("a screenshot comes back as its bytes, and any other reply than a 200 is the service's error")
  func aScreenshotIsItsBytes() async throws {
    let picture = Data([0x89, 0x50, 0x4E, 0x47])
    let shown = try client(.reply(status: 200, body: picture))
    #expect(try await shown.screenshot(name: "1700000005000000000-capture.png", full: true) == picture)
    let refusal = Data(#"{"ok": false, "code": "bad-request", "error": "No such screenshot."}"#.utf8)
    let refused = try client(.reply(status: 404, body: refusal))
    await #expect(throws: ServiceError("No such screenshot.", code: "bad-request")) {
      try await refused.screenshot(name: "nope.png", full: false)
    }
  }
}
