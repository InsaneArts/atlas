import Foundation
import Testing

@testable import AtlasApp

@Suite("PanelToggle")
struct PanelToggleTests {
  @Test("pressing the item opens the panel when it has never closed or closed a while ago")
  func aPressOpensTheClosedPanel() {
    #expect(PanelToggle.opens(closedAt: nil, now: 100))
    #expect(PanelToggle.opens(closedAt: 100, now: 100.5))
    #expect(PanelToggle.opens(closedAt: 100, now: 160))
  }

  @Test("the press that closed the panel through a click elsewhere does not open it again")
  func theClosingPressDoesNotReopen() {
    #expect(!PanelToggle.opens(closedAt: 100, now: 100))
    #expect(!PanelToggle.opens(closedAt: 100, now: 100.05))
    #expect(!PanelToggle.opens(closedAt: 100, now: 100.29))
  }
}
