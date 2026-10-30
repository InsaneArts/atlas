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

  @Test("the incoming list stays hidden until the outgoing list finishes departing")
  func departurePrecedesArrival() {
    let handoff = PaneHandoff()
    handoff.request(.activity)
    #expect(handoff.shown == .activity)
    handoff.request(.apps)
    #expect(handoff.shown == nil)
    #expect(handoff.departing == .activity)
    handoff.departed(.activity)
    #expect(handoff.shown == .apps)
    #expect(handoff.departing == nil)
  }

  @Test("a newer navigation cancels the pending arrival and stale completions do nothing")
  func navigationCancelsPendingArrivals() {
    let handoff = PaneHandoff()
    handoff.request(.activity)
    handoff.request(.apps)
    handoff.request(.screenshots)
    #expect(handoff.shown == nil)
    handoff.departed(.apps)
    #expect(handoff.shown == nil)
    handoff.departed(.activity)
    #expect(handoff.shown == .screenshots)
    handoff.departed(.activity)
    #expect(handoff.shown == .screenshots)
  }

  @Test("reversing navigation cancels departure and the Screen cancels a pending list")
  func reversalAndScreenCancelPendingLists() {
    let handoff = PaneHandoff()
    handoff.request(.activity)
    handoff.request(.apps)
    handoff.request(.activity)
    #expect(handoff.shown == .activity)
    #expect(handoff.departing == nil)
    handoff.departed(.activity)
    #expect(handoff.shown == .activity)
    handoff.request(.apps)
    handoff.request(.screen)
    handoff.departed(.activity)
    #expect(handoff.shown == nil)
  }
}
