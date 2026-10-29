import AtlasService
import AtlasStatus
import Foundation

/// The main window's words for host sessions that depend on what it holds: a session's line in the sessions column
/// and its menu, Activity's groups, the Screenshots count, and a session's facts. Fixed copy, such as the column's
/// headings, the groups' names and the recovery line's buttons, stays in AtlasUI's views; the words the panel shares
/// are ``Words``'.
extension ViewerWords {
  /// The person's own lines' group in Activity.
  static let youGroup = "You"
  /// The commands no host named, gathered as one session.
  static let terminalSession = SessionHost.terminal
    .name(label: "", session: "")
  /// A session nothing names any longer, as after its record was pruned.
  static let unnamedSession = "Another session"
  /// The reason under the action that may not have finished.
  static let unfinishedReason =
    "\(Words.leadingCapital(Words.mayNotHaveFinished)). Every session waits until you recover."
  /// One session's Activity holds nothing yet: the sentence.
  static let noSessionActivityMessage = "What this session does in the helper shows up here."
  /// One session's Screenshots holds nothing: the sentence.
  static let noSessionShotsMessage = "Screenshots this session takes show up here."
  /// The host fact's name.
  static let hostFact = "Host"
  /// The folder fact's name.
  static let folderFact = "Folder"
  /// The start fact's name.
  static let startedFact = "Started"
  /// The last seen fact's name.
  static let lastSeenFact = "Last seen"
  /// The ended fact's name.
  static let endedFact = "Ended"
  /// The fact's name while the sessions' list cannot be read.
  static let sessionsFact = "Sessions"
  /// The fact's value while the sessions' list cannot be read, so what its facts say may be out of date.
  static let sessionsUnread = "Couldn't read sessions; this may be out of date"
  /// The Terminal session's host, which no host named.
  static let noHost = "None named"
  /// An app the session opened, open in the helper now.
  static let appOpen = "Open"
  /// An app the session opened from a copy Atlas made, open now.
  static let appOpenCopy = "Open · a copy"
  /// An app the session opened, closed since.
  static let appClosed = "Closed"
  /// A plan whose phase is not known.
  static let plan = "A plan"

  /// An ended session's line, as its row in the sidebar says it.
  ///
  /// - Parameters:
  ///   - record: The session, as `sessions.json` lists it.
  ///   - now: The time now.
  /// - Returns: "Ended 20 min ago", or "Last seen 2 h ago" for one that only went quiet.
  static func endedSubtitle(_ record: HostSessionRecord, now: Date) -> String {
    let line: SessionLine = record.endedAt == nil ? .lastSeen(at: record.lastSeenAt) : .ended(at: record.endedAt)
    return Words.sessionLine(line, now: now)
  }

  /// Idle, with how long ago it last did something when that is known.
  ///
  /// - Parameter age: How long ago, in seconds.
  /// - Returns: "Idle · 4 min ago", or "Idle".
  static func idle(_ age: Double?) -> String {
    let word = Words.sessionState(.idle)
    return age.map { "\(word) · \(Words.ago(max(0, $0)))" } ?? word
  }

  /// Show all, under a group that holds more lines than it lists.
  ///
  /// - Parameter count: How many lines it holds.
  /// - Returns: "Show all 14".
  static func showAll(_ count: Int) -> String {
    "Show all \(count)"
  }

  /// Show all as VoiceOver reads it, naming whose lines it shows.
  ///
  /// - Parameters:
  ///   - count: How many lines the group holds.
  ///   - name: The group's name.
  /// - Returns: "Show all 14 from Codex · atlas-landing".
  static func showAll(_ count: Int, from name: String) -> String {
    "Show all \(count) from \(name)"
  }

  /// How many screenshots one session took, beside the Screenshots title.
  ///
  /// - Parameters:
  ///   - count: How many.
  ///   - name: The session's name.
  /// - Returns: "6 by Claude Code · understudy", or "None by …".
  static func shotsBy(_ count: Int, name: String) -> String {
    count == 0 ? "None by \(name)" : "\(count) by \(name)"
  }

  /// The copies of an app Atlas made for a session, as their fact's name.
  ///
  /// - Parameters:
  ///   - app: The app's name.
  ///   - count: How many copies of it.
  /// - Returns: "Copy of Opie", or "Copies of Opie" for more than one.
  static func copyOf(_ app: String, count: Int) -> String {
    count > 1 ? "Copies of \(app)" : "Copy of \(app)"
  }

  /// Where a plan stands, in its fact.
  ///
  /// - Parameter plan: The plan.
  /// - Returns: "Applied · can roll back", "Applied", "Applying", "Rolling back", or "A plan" when it is not known.
  static func planPhase(_ plan: HostSessionPlan) -> String {
    switch plan.phase {
    case .applied: plan.canRollBack ? "Applied · can roll back" : "Applied"
    case .applying: "Applying"
    case .rollingBack: "Rolling back"
    case .unknown: Self.plan
    }
  }
}
