import AtlasKit
import Testing

@testable import AtlasApp

@Suite("Mode+Word")
struct ModeWordTests {
  // The app's own word for each row, when the service gives none, is Mode+StatusSummary's "every overview state".
  @Test("a state reads in the service's word over the app's own")
  func serviceWord() throws {
    let worded = try status(["overview": ["mode": "agent", "state": "working", "label": "Busy"]])
    #expect(Mode.of(worded.overview).word == "Busy")
  }
}
