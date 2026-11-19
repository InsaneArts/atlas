import Foundation
import Testing

@testable import AtlasService

@Suite("ViewerClient Stop")
struct ViewerClientStopTests {
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

  @Test("a 409 with one desktop stopped and one failed reads as one stopped and one failed, the failed one named")
  func aFailedStopOfEveryDesktopKeepsEachDesktopsOutcome() async throws {
    let refusal = Data(
      #"""
      {"ok": false, "code": "io", "error": "work could not be told", "uncertain": false,
       "desktops": [
         {"desktop": "main", "uncertain": false, "reply": {"ok": true, "held": true}},
         {"desktop": "work", "uncertain": true,
          "reply": {"ok": false, "code": "io", "error": "work could not be told"}}
       ]}
      """#.utf8
    )
    let stopping = try client(.reply(status: 409, body: refusal))
    let outcome = try await stopping.setStopped(true)
    #expect(outcome.stopped == ["main"])
    #expect(outcome.failed == ["work"])
    #expect(outcome.uncertain == ["work"])
    #expect(!outcome.held)
    #expect(outcome.warnings == ["work could not be told"])
  }

  @Test("a refused Stop that names no desktop's outcome is still the service's error")
  func aRefusedStopWithoutEachDesktopIsItsError() async throws {
    let refusal = Data(#"{"ok": false, "code": "io", "error": "main could not be told"}"#.utf8)
    let stopping = try client(.reply(status: 409, body: refusal))
    await #expect(throws: ServiceError("main could not be told", code: "io")) {
      try await stopping.setStopped(true)
    }
  }
}
