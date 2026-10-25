import AtlasUI
import Testing

@testable import AtlasApp

@Suite("PanelResize")
struct PanelResizeTests {
  @Test(
    "the panel's height eases over a quarter second while it shows, and lands at once while closed, under Reduce Motion or with easing off"
  )
  func theHeightEasesOnlyWhileShown() {
    let eased = PanelResize.eased(0.25)
    #expect(PanelResize.change(isShown: true, reduceMotion: false, eases: true) == eased)
    #expect(PanelResize.change(isShown: false, reduceMotion: false, eases: true) == .instant, "opening shows it whole")
    #expect(PanelResize.change(isShown: true, reduceMotion: true, eases: true) == .instant, "Reduce Motion")
    #expect(PanelResize.change(isShown: true, reduceMotion: false, eases: false) == .instant, "the fallback")
  }
}
