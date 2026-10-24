#if DEBUG
  import AtlasService
  import AtlasServiceTesting
  import Foundation
  import Testing

  @testable import AtlasApp
  import AtlasUI

  extension ViewerPreviewStatesTests {
    /// The Activity tab's samples' facts, by the sample's name: times on a German clock in UTC, for
    /// deterministic timestamps.
    ///
    /// - Returns: What the presenter shows for each.
    /// - Throws: A decoding error, which a fixed reply never gives.
    func activitySamples() throws -> [String: ViewerState] {
      let working = try reported("agent", state: "working", activity: Self.working)
      let idle = try reported("observing", state: "idle")
      return [
        "Activity": shown(working) { snapshot in
          Self.onTab(.activity, at: Self.activityReadAt, &snapshot)
          snapshot.panes.activity.log = Self.activityLog
          snapshot.panes.activity.readAt = Self.activityReadAt
          snapshot.panes.shots.shots = Self.shots
          Self.keepThumbnails(&snapshot)
        },
        "Activity, nothing yet": shown(try reported("observing", state: "idle", activity: [])) { snapshot in
          Self.onTab(.activity, at: Self.activityReadAt, &snapshot)
          snapshot.panes.activity.log = ActivityLog()
          snapshot.panes.activity.readAt = Self.activityReadAt
        },
        "Activity, paused": shown(idle) { snapshot in
          Self.onTab(.activity, at: Self.activityReadAt, &snapshot)
          snapshot.session.paused = true
          snapshot.panes.activity.log = Self.activityLog
          snapshot.panes.activity.readAt = Self.activityReadAt
          snapshot.panes.shots.shots = Self.shots
          Self.keepThumbnails(&snapshot)
        },
      ]
    }

    /// 10:31:42 on 22 October 2026, in UTC: when the Activity sample's log is read.
    static let activityReadAt = Date(timeIntervalSince1970: 1_792_665_102)

    /// The Activity sample's log, newest first, read at 10:31:42.
    static let activityLog = ActivityLog(entries: [
      ActivityLog.Entry(id: "38", says: "turning Show Center off in Opie", outcome: .running, ageSeconds: 0),
      ActivityLog.Entry(id: "37", says: "scrolled in Opie", ageSeconds: 4),
      ActivityLog.Entry(id: "36", says: "clicked Ring in Opie", ageSeconds: 9),
      ActivityLog.Entry(id: "35", says: "took a screenshot", ageSeconds: 22, operation: "screenshot"),
      ActivityLog.Entry(id: "34", says: "opened Opie", ageSeconds: 30, operation: "open"),
      ActivityLog.Entry(id: "33", says: "gave back control", actor: .you, ageSeconds: 122, operation: "release"),
      ActivityLog.Entry(id: "32", says: "clicked in Opie", actor: .you, ageSeconds: 140),
      ActivityLog.Entry(id: "31", says: "clicked in Opie", actor: .you, ageSeconds: 150),
      ActivityLog.Entry(id: "30", says: "clicked in Opie", actor: .you, ageSeconds: 157),
      ActivityLog.Entry(
        id: "29",
        says: "couldn't click Show Centre in Opie",
        outcome: .failed,
        ageSeconds: 192,
        reason: "No element is named Show Centre"
      ),
      ActivityLog.Entry(id: "28", says: "ran a command", ageSeconds: 224, operation: "run"),
    ])
  }
#endif
