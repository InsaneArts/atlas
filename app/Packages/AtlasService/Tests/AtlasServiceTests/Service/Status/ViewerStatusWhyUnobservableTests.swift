import AtlasServiceTesting
import Testing

@testable import AtlasService

@Suite("ViewerStatus+WhyUnobservable")
struct ViewerStatusWhyUnobservableTests {
  @Test("a status that can be observed has no reason")
  func observableHasNoReason() throws {
    #expect(try status(["canObserve": true]).unobservable == nil)
  }

  @Test("a helper that answers says what to do next, else the service's error, else nothing for the app to word")
  func anAnsweringHelperSaysWhatToDo() throws {
    let steps = try status(["nextSteps": ["Allow Screen Recording.", "Then wait."], "overview": ["mode": "blocked"]])
    #expect(steps.unobservable == .said("Allow Screen Recording. Then wait."))
    #expect(try status(["error": "No session", "overview": ["mode": "blocked"]]).unobservable == .said("No session"))
    #expect(try status(["overview": ["mode": "blocked"]]).unobservable == .said(nil))
  }

  @Test("a silent helper is explained by its overview's reason, never the commands its steps name")
  func aSilentHelperUsesItsReason() throws {
    let silent = try status([
      "nextSteps": ["Run atlas init."],
      "overview": ["mode": "unavailable", "reason": "Atlas isn't answering in the helper."],
    ])
    #expect(silent.unobservable == .silent("Atlas isn't answering in the helper."))
    #expect(try status(["overview": ["mode": "unavailable"]]).unobservable == .silent(nil))
    let failed = try status(["error": "Socket closed", "overview": ["mode": "unavailable"]])
    #expect(failed.unobservable == .said("Socket closed"), "a silent helper's own error is said as it is")
  }
}
