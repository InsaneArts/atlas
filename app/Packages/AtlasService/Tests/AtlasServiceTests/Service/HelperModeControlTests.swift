import Testing

@testable import AtlasService

@Suite("HelperMode+Control")
struct HelperModeControlTests {
  @Test(
    "the helper is under this viewer's control only while it holds a lease and the helper answers with nothing to recover",
    arguments: [
      (HelperMode.controlling, true, true),
      (.observing, true, true),
      (.controlling, false, false),
      (.recovering, true, false),
      (.loggedOut, true, false),
      (.unavailable, true, false),
    ] as [(HelperMode, Bool, Bool)]
  )
  func isControlled(mode: HelperMode, holdingControl: Bool, controlled: Bool) {
    #expect(mode.isControlled(byThisViewer: holdingControl) == controlled)
  }
}
