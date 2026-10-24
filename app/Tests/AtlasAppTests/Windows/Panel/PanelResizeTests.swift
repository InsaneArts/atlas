import AtlasUI
import Testing

@testable import AtlasApp

@Suite("PanelResize")
struct PanelResizeTests {
  @Test(
    "the panel's height eases on the state step while it shows, and lands at once while closed, under Reduce Motion or with easing off"
  )
  func theHeightEasesOnlyWhileShown() {
    let eased = PanelResize.eased(Motion.Duration.state.seconds)
    #expect(PanelResize.change(isShown: true, reduceMotion: false, eases: true) == eased)
    #expect(PanelResize.change(isShown: false, reduceMotion: false, eases: true) == .instant, "opening shows it whole")
    #expect(PanelResize.change(isShown: true, reduceMotion: true, eases: true) == .instant, "Reduce Motion")
    #expect(PanelResize.change(isShown: true, reduceMotion: false, eases: false) == .instant, "the fallback")
    #expect(Motion.Duration.state.seconds == 0.3, "Motion.state's length")
  }
}
