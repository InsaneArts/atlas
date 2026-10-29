import AtlasService
import AtlasStatus
import Foundation

/// The sidebar's words that depend on what the window holds: what each session last did under its name, and the
/// helper's health at its foot. Its fixed copy, such as the sections' headings and All sessions, stays in AtlasUI's
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

  /// The helper's line while it can act.
  ///
  /// - Parameter name: The helper's short name.
  /// - Returns: "Helper ready · atlas".
  static func helperReady(_ name: String) -> String {
    "Helper ready · \(name)"
  }

  /// The helper's line while some of its checks do not pass.
  ///
  /// - Parameter checks: How many do not.
  /// - Returns: "Helper needs setup · 2 checks", or "Helper needs setup" when none is named.
  static func helperNeedsSetup(_ checks: Int) -> String {
    switch checks {
    case 0: "Helper needs setup"
    case 1: "Helper needs setup · 1 check"
    default: "Helper needs setup · \(checks) checks"
    }
  }

  /// The helper's line while it needs the person to recover it.
  static let helperNeedsYou = "Helper needs you"
  /// The helper's line while it is logged out.
  static let helperLoggedOut = "Helper logged out"
  /// The helper's line before the first reading, or while its picture is lost.
  static let helperConnecting = "Connecting to the helper"
}
