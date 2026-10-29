import AtlasService
import AtlasStatus
import Foundation

/// The main window's words for host sessions that depend on what it holds: the sessions button and its menu, a
/// session's line there and in Details, Activity's groups, the Screenshots count, and Details' facts. Fixed copy, such
/// as the groups' names and the recovery line's buttons, stays in AtlasUI's views; the words the panel shares are
/// ``Words``'.
extension ViewerWords {
  /// The sessions button while every session's work shows.
  static let allSessionsButton = "All sessions"
  /// The sessions button's tooltip.
  static let sessionsHelp = "Choose whose work to show"
  /// The menu's first item, which shows every session's work.
  static let allSessionsItem = "All Sessions"
  /// The header over the ended sessions in the menu.
  static let endedHeader = "Ended"
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
  /// The fact's value while the sessions' list cannot be read, so what Details shows may be out of date.
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

  /// A session's state and line as its menu item and Details say them: what it does, what it waits for, how long it
  /// has been idle, or when it ended.
  ///
  /// - Parameters:
  ///   - session: The session, as the status lists it.
  ///   - line: Its line, as AtlasStatus decides it.
  ///   - now: The time now.
  /// - Returns: "Acting · has the helper", "Waiting for the helper · 8 s", "Waits until you recover", "Needs you · a
  ///   click may not have finished", "Idle · 4 min ago", "Ended 20 min ago" or "Last seen 2 h ago".
  static func sessionSubtitle(_ session: ViewerStatus.HostSession, line: SessionLine, now: Date) -> String {
    switch line {
    case .acting:
      return stated(.acting, Words.sessionLine(.acting(says: nil), now: now))
    case .waitingForHelper, .waitingForRecovery, .ended, .lastSeen:
      return Words.sessionLine(line, now: now)
    case .mayNotHaveFinished:
      return stated(.needsYou, Words.sessionLine(line, now: now))
    case .last(_, let age):
      return idle(age ?? session.lastSeenAt.map { now.timeIntervalSince($0) })
    case .none:
      return idle(session.lastSeenAt.map { now.timeIntervalSince($0) })
    }
  }

  /// A state's word, then its line, as one subtitle.
  ///
  /// - Parameters:
  ///   - state: The session's state.
  ///   - line: Its line, as the panel says it.
  /// - Returns: "Needs you · a click may not have finished".
  private static func stated(_ state: HostSessionState, _ line: String) -> String {
    "\(Words.sessionState(state)) · \(Words.leadingLower(line))"
  }

  /// An ended session's line, as its menu item and Details say it.
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

  /// See all, under a session's newest screenshots in Details.
  ///
  /// - Parameter count: How many screenshots it took.
  /// - Returns: "See all 6".
  static func seeAll(_ count: Int) -> String {
    "See all \(count)"
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

  /// When a plan was made, as its fact's value.
  ///
  /// - Parameter time: The time, on the person's clock.
  /// - Returns: "Made at 10:02".
  static func madeAt(_ time: String) -> String {
    "Made at \(time)"
  }

  /// Where a plan stands, as its fact's name.
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
