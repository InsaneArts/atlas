import AtlasKit
import Testing

@testable import AtlasApp

@Suite("HelperRecovery")
struct HelperRecoveryTests {
  @Test("a late recovery is reported as a value the app words as Still recovering…")
  func lateRecovery() {
    let late = ServiceError("No answer", code: ViewerClient.stillRecoveringCode)
    #expect(HelperRecovery.notice(late) == "The helper is still recovering. This can take a minute.")
    #expect(HelperRecovery.notice(ServiceError("Recovery failed")) == "Recovery failed.", "any other is its own words")
  }
}
