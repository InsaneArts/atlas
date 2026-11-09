import Foundation

/// `activity.json` as the service writes it, scrubbed, with one row whose every field but its id is null.
let activityAllNull = Data(
  #"""
  {"ok": true, "activity": [{"id": "9f2c41d07a3b55e1", "says": null, "ok": null, "started": null, "actor": null,
  "ageSeconds": null, "reason": null, "operation": null, "count": null}]}
  """#.utf8
)

/// `activity.json` as the service writes it, scrubbed, with one row.
///
/// - Parameters:
///   - actor: The row's `actor`.
///   - ok: The row's `ok`.
///   - started: The row's `started`.
/// - Returns: The reply's body.
func activity(actor: String = "agent", ok: Bool = true, started: Bool = false) -> Data {
  Data(
    """
    {"ok": true, "activity": [{"id": "9f2c41d07a3b55e1", "says": "typed in TextEdit", "ok": \(ok),
    "started": \(started), "actor": "\(actor)", "ageSeconds": 3, "reason": null, "operation": "input", "count": 2}]}
    """.utf8
  )
}

/// `screenshots.json` as the service writes it, scrubbed: one screenshot whose size did not read.
let screenshotWithNoSize = Data(
  #"{"ok": true, "screenshots": [{"name": "1700000005000000000-capture.png", "takenAt": 1700000005000}]}"#.utf8
)
