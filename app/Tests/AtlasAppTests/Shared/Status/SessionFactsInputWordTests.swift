import AtlasServiceTesting
import AtlasStatus
import Testing

@testable import AtlasApp

@Suite("SessionFacts.Input+Word")
struct SessionFactsInputWordTests {
  @Test("an input service that does not answer reads Not answering")
  func notAnswering() throws {
    let facts = StatusReading(try status([:]))
      .facts(holdingControl: false)
    #expect(facts.input == .notAnswering)
    #expect(facts.input.word == "Not answering")
  }

  @Test("input that is ready reads Ready, and input that answers but isn't ready reads Not ready")
  func readyAndNotReady() {
    #expect(SessionFacts.Input.ready.word == "Ready")
    #expect(SessionFacts.Input.notReady.word == "Not ready")
  }
}
