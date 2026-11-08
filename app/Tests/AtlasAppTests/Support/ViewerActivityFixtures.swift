import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation

@testable import AtlasApp

/// Activity snapshots and projections shared by viewer tests.
enum ViewerActivityFixtures {
  /// The Activity tab over `log` while Claude Code acts, Codex waits and Cursor is idle, read at the snapshot's time.
  ///
  /// - Parameters:
  ///   - log: The log read.
  ///   - sessions: The status's sessions; the three by default.
  ///   - mode: The helper's mode.
  ///   - state: The state the service shows for it.
  ///   - pending: Actions of unknown outcome.
  /// - Returns: The snapshot.
  /// - Throws: A decoding error, which a fixed reply never gives.
  static func onActivity(
    _ log: [ActivityLog.Entry],
    sessions: [[String: Any]] = [
      SessionReplies.claudeActing, SessionReplies.codexWaiting(), SessionReplies.cursorIdle,
    ],
    mode: String = "agent",
    state: String = "working",
    pending: Int = 0
  ) throws -> ViewerSnapshot {
    var snapshot = try ViewerSnapshot.listing(sessions, mode: mode, state: state, pending: pending)
    snapshot.session.place = .activity
    snapshot.date = ActivityFixtures.readAt
    snapshot.timeZone = ActivityFixtures.calendar.timeZone
    snapshot.locale = Locale(identifier: "de_DE")
    snapshot.panes.activity.log = ActivityLog(entries: log)
    snapshot.panes.activity.readAt = snapshot.date
    return snapshot
  }

  /// Claude Code's session key.
  static let claude = "3f9a1c07b2e4"
  /// Codex's session key.
  static let codex = "8be20c4d1a77"

  /// An agent's four steps in TextEdit, newest first: ⌘S pressed, Name typed into, Save clicked, TextEdit opened.
  static let textEditLog = [
    ActivityLog.Entry(
      id: "k1",
      says: "pressed ⌘S in TextEdit",
      ageSeconds: 0,
      verb: "key",
      app: "TextEdit",
      label: "⌘S",
      keys: [["⌘", "S"]]
    ),
    ActivityLog.Entry(
      id: "t1",
      says: "typed into Name in TextEdit",
      ageSeconds: 5,
      verb: "type",
      app: "TextEdit",
      label: "Name"
    ),
    ActivityLog.Entry(
      id: "c1",
      says: "clicked Save in TextEdit",
      ageSeconds: 10,
      verb: "click",
      app: "TextEdit",
      label: "Save"
    ),
    ActivityLog.Entry(
      id: "o1",
      says: "opened TextEdit",
      ageSeconds: 20,
      operation: "open",
      verb: "open",
      app: "TextEdit"
    ),
  ]

  /// Six of Claude Code's rows in Opie, one of Codex's in Notes, the person's, and one no host named, newest first.
  static var mixedLog: [ActivityLog.Entry] {
    (0..<6)
      .map {
        ActivityLog.Entry(
          id: "claude-\($0)",
          says: "clicked Item \($0) in Opie",
          ageSeconds: Double($0),
          session: claude,
          verb: "click",
          app: "Opie",
          label: "Item \($0)"
        )
      } + [
        ActivityLog.Entry(
          id: "codex",
          says: "opened Notes",
          ageSeconds: 10,
          operation: "open",
          session: codex,
          verb: "open",
          app: "Notes"
        ),
        ActivityLog.Entry(
          id: "you",
          says: "clicked in Opie",
          actor: .you,
          ageSeconds: 20,
          verb: "click",
          app: "Opie"
        ),
        ActivityLog.Entry(id: "terminal", says: "ran a command", ageSeconds: 30, operation: "run"),
      ]
  }

  /// The sessions a pane lists.
  ///
  /// - Parameter activity: The pane.
  /// - Returns: Its sessions, in order.
  static func sessions(_ activity: ViewerActivityState) -> [ViewerOutlineSession] {
    activity.days.flatMap(\.sessions)
  }

  /// The runs a pane lists.
  ///
  /// - Parameter activity: The pane.
  /// - Returns: Its runs, in order.
  static func runs(_ activity: ViewerActivityState) -> [ViewerOutlineRun] {
    activity.days
      .flatMap(\.sessions)
      .flatMap(\.runs)
  }

  /// The steps a pane lists in its opened runs.
  ///
  /// - Parameter activity: The pane.
  /// - Returns: Its steps, in order.
  static func steps(_ activity: ViewerActivityState) -> [ViewerActivityStep] {
    runs(activity)
      .flatMap(\.steps)
  }

  /// The pane with every run opened, as the person would open each to read its steps.
  ///
  /// - Parameter snapshot: The facts.
  /// - Returns: The pane.
  static func opened(_ snapshot: ViewerSnapshot) -> ViewerActivityState {
    var snapshot = snapshot
    for moment in ViewerPresenter.activityMoments(snapshot)?.moments ?? [] {
      snapshot.panes.activity.choices[moment.id] = true
    }
    return ViewerPresenter.activity(snapshot)
  }

  /// The picture sheet opened on the screen just before a row, as its step's picture button opens it.
  ///
  /// - Parameters:
  ///   - snapshot: The facts.
  ///   - row: The row that kept the picture.
  ///   - side: Which of its pictures shows; before by default.
  /// - Returns: The sheet, or nil when no run holds the row.
  static func sheet(
    _ snapshot: ViewerSnapshot,
    row: String,
    side: ViewerPictureSide = .before
  ) -> ViewerPictureSheet? {
    var snapshot = snapshot
    let run = ViewerPresenter.activityMoments(snapshot)?.moments
      .first { $0.runs.contains { line in line.members.contains { $0.id == row } } }
    guard let run else { return nil }
    snapshot.panes.activity.sheet = ViewerPictureFocus(run: run.id, item: .before(row), everyStep: true, side: side)
    return ViewerPresenter.activity(snapshot).sheet
  }
}
