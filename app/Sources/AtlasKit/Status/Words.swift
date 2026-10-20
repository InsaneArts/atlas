/// Words for times, operations and facts, shared by the panel, the viewer and the setup window, each written once.
///
/// One punctuation rule holds for every word AtlasKit makes: a line a window shows on its own as a sentence (a hint, a
/// notice, a reason to wait) ends with its full stop, and a label, a title, a mode or a fact's value has none.
package enum Words {
  /// What a logged-out helper means, and the way back.
  package static let loggedOut =
    "Agents can't use it until you log in to it again. Then switch back here; Atlas carries on by itself."
  /// A fact whose value is not known yet: the same dash in the panel and the viewer.
  package static let unknown = "—"
  /// The helper's display, as the panel's and the viewer's facts name it.
  package static let displayFact = "Display"
  /// The helper's reason before the service has given one.
  package static let connecting = "Connecting to the helper"
  /// Another viewer holds control: the mode's label and the reason under it.
  package static let anotherViewer = "Another viewer has control"
  /// While the helper is recovered, from the panel or the viewer.
  package static let recovering = "Recovering the helper…"
  /// Once recovery ran without restarting the helper, from the panel or the viewer.
  package static let readyAgain = "The helper is ready again."
  /// Once recovery restarted the helper, which ended any control.
  package static let recovered = "The helper was recovered. Take control again when ready."
  /// While the screen locks so the person can switch accounts, from the panel or the setup window.
  package static let locking = "Locking the screen…"

  /// Seconds ago as a short relative time.
  ///
  /// - Parameter seconds: How long ago.
  /// - Returns: "just now", or seconds, minutes or hours ago.
  package static func ago(_ seconds: Double) -> String {
    if seconds < 5 { return "just now" }
    if seconds < 90 { return "\(Int(seconds.rounded())) s ago" }
    if seconds < 5400 { return "\(Int((seconds / 60).rounded())) min ago" }
    return "\(Int((seconds / 3600).rounded())) h ago"
  }

  /// What an operation did. No payload is ever recorded.
  ///
  /// - Parameter row: The recorded operation.
  /// - Returns: What it did, such as "ran a command", or that it failed.
  package static func describe(_ row: ViewerStatus.Activity) -> String {
    if !row.ok { return "\(row.operation) failed" }
    let count = row.actionCount ?? 0
    switch row.operation {
    case "input" where count > 0: return "sent \(count) action\(count == 1 ? "" : "s")"
    case "run": return "ran a command"
    case "restart": return "recovered"
    case "logout": return "logged out the helper"
    default: return row.operation
    }
  }

  /// "1512 × 982", or nil when unknown.
  ///
  /// - Parameter size: A display's size, in points.
  /// - Returns: Width × height in whole points, or `nil` for no size or an empty one.
  package static func size(_ size: ViewerStatus.Size?) -> String? {
    guard let size, size.width > 0, size.height > 0 else { return nil }
    return "\(Int(size.width.rounded())) × \(Int(size.height.rounded()))"
  }

  /// How fresh the picture is, from the age of the last one.
  ///
  /// - Parameter age: The last picture's age, in seconds; `nil` before the first.
  /// - Returns: "Live", how long ago it updated, or that it is stale.
  package static func freshness(_ age: Double?) -> String {
    guard let age, age.isFinite else { return "Waiting for the first picture" }
    if age < Freshness.live { return "Live" }
    if age < Freshness.stale { return "Updated \(Int(age.rounded())) s ago" }
    return "Picture is stale"
  }
}
