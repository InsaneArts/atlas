import Testing

@testable import AtlasUI

@Suite("Pane handoff")
@MainActor
struct PaneHandoffTests {
  @Test("the initial list is admitted before mounting and the Screen starts with no retained list")
  func theInitialDestinationNeedsNoHandoff() {
    let list = PaneHandoff(.activity)
    #expect(list.shown == .activity)
    #expect(list.departing == nil)
    list.request(.activity)
    #expect(list.shown == .activity)
    #expect(PaneHandoff(.screen).shown == nil)
  }

  @Test("leaving Activity for the Screen hides it until its departure finishes")
  func departureEndsWithNothingShown() {
    let handoff = PaneHandoff()
    handoff.request(.activity)
    #expect(handoff.shown == .activity)
    handoff.request(.screen)
    #expect(handoff.shown == nil)
    #expect(handoff.departing == .activity)
    handoff.departed(.activity)
    #expect(handoff.shown == nil)
    #expect(handoff.departing == nil)
  }

  @Test("going back to Activity while it departs reverses the departure, and stale completions do nothing")
  func reversalCancelsDeparture() {
    let handoff = PaneHandoff()
    handoff.request(.activity)
    handoff.request(.screen)
    handoff.request(.activity)
    #expect(handoff.shown == .activity)
    #expect(handoff.departing == nil)
    handoff.departed(.activity)
    #expect(handoff.shown == .activity)
  }
}
