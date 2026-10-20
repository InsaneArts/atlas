import AtlasKit
import Testing

@testable import AtlasApp

@Suite("LogOutStage")
struct LogOutStageTests {
  @Test(
    "a new status keeps a confirmation while the mode allows logging out, and Log Out Anyway while it allows forcing",
    arguments: [
      (LogOutStage.idle, HelperMode.observing, LogOutStage.idle),
      (.confirming, .observing, .confirming),
      (.confirming, .agent, .idle),
      // Recovering allows only a forced log-out: the confirmation goes, the offer to force stays.
      (.confirming, .recovering, .idle),
      (.held, .recovering, .held),
      (.held, .agent, .idle),
    ] as [(LogOutStage, HelperMode, LogOutStage)]
  )
  func kept(stage: LogOutStage, mode: HelperMode, kept: LogOutStage) {
    #expect(stage.kept(in: mode) == kept)
  }
}
