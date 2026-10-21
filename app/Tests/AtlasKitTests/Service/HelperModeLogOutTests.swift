import Testing

@testable import AtlasKit

@Suite("HelperMode+LogOut")
struct HelperModeLogOutTests {
  @Test(
    "log out is allowed only while the helper answers with nothing to settle; a forced log-out also while recovering",
    arguments: [
      (HelperMode.observing, true, true),
      (.controlling, true, true),
      (.blocked, true, true),
      (.stopped, true, true),
      (.recovering, false, true),
      (.agent, false, false),
      (.unavailable, false, false),
      (.loggedOut, false, false),
    ]
  )
  func logOut(mode: HelperMode, allowed: Bool, forced: Bool) {
    #expect(mode.allowsLogOut == allowed)
    #expect(mode.allowsForcedLogOut == forced)
  }
}
