import AtlasService
import AtlasStatus
import Foundation

/// The sidebar's words that depend on what the window holds: what each session last did under its name, and the
/// helper's row at its foot. Its fixed copy, such as the sections' headings and All sessions, stays in AtlasUI's
/// views.
extension ViewerWords {
  /// What a live session last did, under its name in the sidebar: what it does now, what it waits for, what it did
  /// last and how long ago, or how long it has been idle.
  ///
  /// - Parameters:
  ///   - session: The session, as the status lists it.
  ///   - line: Its line, as AtlasStatus decides it.
  ///   - now: The time now.
  /// - Returns: "Clicking Show Center in Opie", "Waiting for the helper · 8 s", "A click may not have finished",
  ///   "Opened Opie · 4 min ago" or "Idle · 4 min ago".
  static func sessionRowLine(_ session: ViewerStatus.HostSession, line: SessionLine, now: Date) -> String {
    switch line {
    case .last(let says, let age):
      let did = Words.leadingCapital(says)
      return age.map { "\(did) · \(Words.ago(max(0, $0)))" } ?? did
    case .none:
      return idle(session.lastSeenAt.map { now.timeIntervalSince($0) })
    case .acting, .waitingForHelper, .waitingForRecovery, .mayNotHaveFinished, .ended, .lastSeen:
      return Words.sessionLine(line, now: now)
    }
  }

  /// The helper's row while it can act.
  static let helperReadyTitle = "Atlas is ready"
  /// The helper's row while some of its checks do not pass, or it is not ready.
  static let helperSetupTitle = "Atlas needs setup"
  /// The helper's row while it needs the person to recover it.
  static let helperNeedsYouTitle = "Atlas needs you"
  /// The helper's row while it is logged out.
  static let helperLoggedOutTitle = "The helper is logged out"
  /// The helper's row before the first reading, or while its picture is lost.
  static let helperConnectingTitle = "Connecting to Atlas…"
  /// What the person does while the helper needs them.
  static let recoverLine = "Recover the helper to go on"

  /// The helper by its short name, with its display when known, under the helper row's title.
  ///
  /// - Parameters:
  ///   - name: The helper's short name; nil before it is known.
  ///   - display: Its display, such as "1440 × 900"; nil when not known.
  /// - Returns: "Helper atlas · 1440 × 900", "Helper atlas", or "Helper".
  static func helperNamed(_ name: String?, display: String? = nil) -> String {
    let named = name.map { "Helper \($0)" } ?? "Helper"
    return display.map { "\(named) · \($0)" } ?? named
  }

  /// How many of the helper's checks do not pass.
  ///
  /// - Parameter checks: How many, one or more.
  /// - Returns: "1 check doesn't pass" or "2 checks don't pass".
  static func checksFailing(_ checks: Int) -> String {
    checks == 1 ? "1 check doesn't pass" : "\(checks) checks don't pass"
  }

  /// What the person does while the helper is logged out.
  ///
  /// - Parameter name: The helper's short name; nil before it is known.
  /// - Returns: "Log in to atlas to go on", or "Log in to the helper to go on".
  static func logInLine(_ name: String?) -> String {
    "Log in to \(name ?? "the helper") to go on"
  }
}
