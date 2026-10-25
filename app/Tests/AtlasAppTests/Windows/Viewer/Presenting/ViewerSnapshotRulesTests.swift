import AtlasService
import AtlasServiceTesting
import AtlasStatus
import Foundation
import Testing

@testable import AtlasApp

@Suite("ViewerSnapshot+Rules")
struct ViewerSnapshotRulesTests {
  /// A viewer that has read `reply` and set its recovery alert aside with Not Now.
  ///
  /// - Parameter reply: The status's JSON fields.
  /// - Returns: The snapshot, its alert set aside.
  /// - Throws: When the reply is not a status, or offers no recovery to set aside.
  private static func setAside(_ reply: [String: Any]) throws -> ViewerSnapshot {
    var snapshot = ViewerSnapshot()
    snapshot.reading.status = try status(reply)
    let offered = try #require(snapshot.offeredRecovery, "the status offers recovery")
    snapshot.session.recoveryLater = RecoveryKey(offered, status: snapshot.reading.status)
    return snapshot
  }

  @Test(
    "a recovery set aside alerts again once its reason or the helper's mode changes while recovery is still offered",
    arguments: [
      ("recovering", "Mouse and keyboard stopped in the helper"),
      ("blocked", "Unknown outcome"),
    ]
  )
  func aChangedRecoveryAlertsAgain(mode: String, reason: String) throws {
    var snapshot = try Self.setAside(["overview": ["mode": "recovering", "reason": "Unknown outcome"]])
    try #require(snapshot.recovery == .setAside, "the same reason stays set aside")
    // Recover fixes what keeps a blocked helper not ready, so it is still offered.
    snapshot.reading.status = try status(["overview": ["mode": mode, "reason": reason, "recoverable": true]])
    let offered = try #require(snapshot.offeredRecovery, "recovery is still offered")
    #expect(snapshot.recovery == .alert(offered))
  }

  @Test("control waits for a fresh picture: one as old as the stale mark is not taken over")
  func aStalePictureIsNotTakenOver() throws {
    var snapshot = ViewerSnapshot()
    snapshot.reading.status = try status(["ready": true, "overview": ["mode": "observing"]])
    snapshot.reading.ready = true
    snapshot.reading.frame = (id: "f", revision: "r", display: .init(width: 4, height: 3))
    snapshot.reading.frameAt = 0
    snapshot.now = Freshness.stale - 1
    try #require(snapshot.canTakeControl, "a fresh picture is taken over")
    snapshot.now = Freshness.stale
    #expect(!snapshot.canTakeControl)
  }

  @Test("a recovery running here shows its alert even when that alert was set aside")
  func aRunningRecoveryOverridesNotNow() throws {
    var snapshot = ViewerSnapshot()
    snapshot.reading.status = try status(["overview": ["mode": "recovering", "reason": "Unknown outcome"]])
    snapshot.session.recovering = true
    let running = try #require(snapshot.offeredRecovery, "a running recovery is offered")
    snapshot.session.recoveryLater = RecoveryKey(running, status: snapshot.reading.status)
    #expect(snapshot.recovery == .alert(running))
  }
}
