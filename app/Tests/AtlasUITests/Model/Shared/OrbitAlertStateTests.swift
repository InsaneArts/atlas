import Testing

@testable import AtlasUI

@Suite("OrbitAlertState")
struct OrbitAlertStateTests {
  @Test("Escape leaves an alert only while its action isn't running, as its buttons wait then too")
  func escapeWaitsWhileBusy() {
    var alert = OrbitAlertState(
      title: "Log out the helper?",
      message: "Its apps quit.",
      buttons: [
        OrbitAlertButton(id: "cancel", title: "Cancel", role: .safeCancel),
        OrbitAlertButton(id: "logOut", title: "Log Out", role: .destructive),
      ]
    )
    #expect(alert.escape == "cancel")
    #expect(alert.escapeNeedsKey, "Cancel takes Return here, so Escape needs a key of its own")
    alert.isBusy = true
    #expect(alert.escape == nil)
  }
}
