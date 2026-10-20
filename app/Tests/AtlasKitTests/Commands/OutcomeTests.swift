import Foundation
import Testing

@testable import AtlasKit

@Suite("Outcome")
struct OutcomeTests {
  @Test("a failure carries the code it names; a success carries none")
  func failureCode() {
    let held = Outcome.parse(
      status: 1,
      stdout: Data(),
      stderr: Data(#"{"ok": false, "code": "logout-held", "error": "Not yet"}"#.utf8)
    )
    #expect(!held.ok)
    #expect(held.code == "logout-held")
    #expect(held.isLogOutHeld)
    #expect(held.message == "Not yet")
    let done = Outcome.parse(status: 0, stdout: Data(#"{"ok": true}"#.utf8), stderr: Data())
    #expect(done.ok && done.code == nil)
    #expect(!done.isLogOutHeld)
  }
}
