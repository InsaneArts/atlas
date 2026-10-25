import Foundation

/// `apps.json` as the service writes it, scrubbed: one app installed outside the Applications folders, so with no
/// icon, its bundle and its window's title unknown.
let appWithNoIcon = Data(
  #"""
  {"ok": true, "apps": [{"pid": 3, "name": "Opie", "bundle": null, "front": true,
  "windows": [{"id": 12, "title": null, "x": 10.0, "y": 40.0, "width": 520.0, "height": 860.0}]}]}
  """#.utf8
)

/// `apps.json` as the service answers when it cannot reach the helper, scrubbed.
let appsUnreachable = Data(#"{"ok": false, "code": "unavailable", "error": "The helper is not running."}"#.utf8)

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
