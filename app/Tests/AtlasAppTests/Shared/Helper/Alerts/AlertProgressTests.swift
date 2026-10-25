import Testing

@testable import AtlasApp

@Suite("AlertProgress")
struct AlertProgressTests {
  @Test("done comes before running, running before a refusal, and a failure while running is its note")
  func precedence() {
    #expect(AlertProgress.of(running: true, failure: "Why.", done: true) == .done)
    #expect(AlertProgress.of(running: true, failure: "Wait.") == .running(note: "Wait."))
    #expect(AlertProgress.of(running: true, failure: nil) == .running(note: nil))
    #expect(AlertProgress.of(running: false, failure: "Why.") == .refused("Why."))
    #expect(AlertProgress.of(running: false, failure: nil) == .asking)
  }
}
