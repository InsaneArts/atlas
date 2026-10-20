import Testing

@testable import AtlasKit

@Suite("ViewerStatus+Unobservable")
struct ViewerStatusUnobservableTests {
  @Test("a status that can be observed has no reason")
  func observableHasNoReason() throws {
    #expect(try status(["canObserve": true]).unobservableReason == nil)
  }

  @Test("a helper that answers says what to do next, else the service's error, else that it is unavailable")
  func anAnsweringHelperSaysWhatToDo() throws {
    let steps = try status(["nextSteps": ["Allow Screen Recording.", "Then wait."], "overview": ["mode": "blocked"]])
    #expect(steps.unobservableReason == "Allow Screen Recording. Then wait.")
    #expect(try status(["error": "No session", "overview": ["mode": "blocked"]]).unobservableReason == "No session")
    #expect(try status(["overview": ["mode": "blocked"]]).unobservableReason == "Helper unavailable")
  }

  @Test("a silent helper is explained in the app's words, never the commands its steps name")
  func aSilentHelperUsesTheAppsWords() throws {
    let silent = try status([
      "nextSteps": ["Run atlas init."],
      "overview": ["mode": "unavailable", "reason": "Atlas isn't answering in the helper."],
    ])
    #expect(silent.unobservableReason == "Atlas isn't answering in the helper.")
    let failed = try status(["error": "Socket closed", "overview": ["mode": "unavailable"]])
    #expect(failed.unobservableReason == "Socket closed", "a silent helper's own error is said as it is")
  }
}
