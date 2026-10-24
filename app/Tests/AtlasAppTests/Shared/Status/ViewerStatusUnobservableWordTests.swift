import AtlasService
import Testing

@testable import AtlasApp

@Suite("ViewerStatus.Unobservable+Word")
struct ViewerStatusUnobservableWordTests {
  @Test(
    "a helper that says nothing of what to do reads Helper unavailable, and a silent one without a reason connecting"
  )
  func fallbacks() {
    #expect(ViewerStatus.Unobservable.said(nil).word == "Helper unavailable")
    #expect(ViewerStatus.Unobservable.silent(nil).word == Words.connecting)
  }

  @Test("the service's own words are said as they are")
  func serviceWords() {
    #expect(ViewerStatus.Unobservable.silent("Atlas isn't answering.").word == "Atlas isn't answering.")
    #expect(ViewerStatus.Unobservable.said("No session").word == "No session")
  }
}
