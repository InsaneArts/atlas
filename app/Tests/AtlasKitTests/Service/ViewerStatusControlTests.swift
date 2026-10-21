import Testing

@testable import AtlasKit

@Suite("ViewerStatus+Control")
struct ViewerStatusControlTests {
  @Test(
    "no viewer is in control when the lease says so, whatever the mode, or, when it does not say, in a mode no lease allows",
    arguments: [
      (false, "controlling", true),
      (true, "observing", false),
      (nil, "observing", true),
      (nil, "controlling", false),
      // Recovering is shown before control, so a lease may still be held.
      (nil, "recovering", false),
    ] as [(Bool?, String, Bool)]
  )
  func noViewerInControl(controlActive: Bool?, mode: String, none: Bool) throws {
    var reply: [String: Any] = ["overview": ["mode": mode]]
    reply["controlActive"] = controlActive
    #expect(try status(reply).noViewerInControl == none)
  }
}
