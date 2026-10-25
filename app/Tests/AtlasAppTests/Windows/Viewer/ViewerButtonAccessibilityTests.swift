import AtlasUI
import Testing

@testable import AtlasApp

@Suite("Viewer button accessibility")
struct ViewerButtonAccessibilityTests {
  @Test("Give Back and both agent actions expose the full menu names regardless of their shortened visible titles")
  func fullActionNames() throws {
    let control = try #require(PreviewStates.viewerControl.top.actions.control)
    #expect(control.title == "Give Back")
    #expect(control.accessibilityLabel == "Give Back Control")
    #expect(control.accessibilityLabel == ViewerWords.giveBackItem)
    #expect(ViewerAgentButton.stop.accessibilityLabel == "Stop Agent")
    #expect(ViewerAgentButton.stop.accessibilityLabel == ViewerWords.stopAgentItem)
    #expect(ViewerAgentButton.resume(isEnabled: true).accessibilityLabel == "Resume Agent")
    #expect(ViewerAgentButton.resume(isEnabled: false).accessibilityLabel == ViewerWords.resumeAgentItem)
  }
}
