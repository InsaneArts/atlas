import Testing

@testable import AtlasKit

@Suite("Mode")
struct ModeTests {
  @Test("each helper mode has its label, tone and mark")
  func modes() {
    #expect(Mode.of(.observing).tone == .ready)
    #expect(Mode.of(.controlling).mark == .filled)
    #expect(Mode.of(.recovering).tone == .attention)
    #expect(Mode.of(.unavailable).label == "Unavailable")
    #expect(Mode.of(.loggedOut).label == "Logged out")
  }

  @Test(
    "a viewer tells its own control from another viewer's, and a logged-out helper never shows control as yours"
  )
  func seen() {
    var overview = ViewerStatus.Overview()
    overview.mode = .controlling
    #expect(Mode.seen(by: overview, holdingControl: true).label == "You have control")
    #expect(Mode.seen(by: overview, holdingControl: false).label == "Another viewer has control")
    overview.mode = .recovering
    #expect(Mode.seen(by: overview, holdingControl: true).label == "Needs you")
    overview.mode = .loggedOut
    #expect(Mode.seen(by: overview, holdingControl: true).label == "Logged out", "no control without a helper")
  }
}
