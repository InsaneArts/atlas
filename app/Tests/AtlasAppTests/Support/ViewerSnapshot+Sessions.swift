import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation

@testable import AtlasApp

extension ViewerSnapshot {
  /// What the main window's controller holds once it has read a status listing these host sessions: the helper in
  /// `mode`, sessions shown while two or more are listed, the one holding the helper named, `pending` actions of
  /// unknown outcome, and a picture on show.
  ///
  /// - Parameters:
  ///   - sessions: The entries, in the service's order.
  ///   - mode: The helper's mode, as the service names it.
  ///   - state: The state the service shows for it.
  ///   - pending: Actions of unknown outcome.
  ///   - activity: The overview's latest actions, newest first; none by default.
  ///   - more: How many more live sessions there are than listed; none by default.
  /// - Returns: The snapshot.
  /// - Throws: A decoding error, which a fixed reply never gives.
  static func listing(
    _ sessions: [[String: Any]],
    mode: String = "agent",
    state: String = "working",
    pending: Int = 0,
    activity: [[String: Any]] = [],
    more: Int = 0
  ) throws -> ViewerSnapshot {
    var overview: [String: Any] = [
      "mode": mode, "state": state, "pending": pending, "sessionsShown": sessions.count > 1, "hostSessions": sessions,
      "activity": activity, "moreSessions": more,
    ]
    overview["holder"] = sessions.first { $0["holdsHelper"] as? Bool == true }?["key"]
    var snapshot = ViewerSnapshot()
    snapshot.reading.status = try status(["ready": true, "canObserve": true, "overview": overview])
    snapshot.reading.hasStatus = true
    snapshot.reading.ready = true
    if let sample = SampleDesktop.picture {
      snapshot.reading.picture = sample
      snapshot.reading.frame = (
        id: sample.id, revision: sample.id, display: ViewerStatus.Size(width: 1440, height: 900)
      )
      snapshot.reading.frameAt = snapshot.now
    }
    return snapshot
  }
}
