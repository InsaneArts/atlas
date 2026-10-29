import AtlasService
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("ViewerPresenter+Activity")
struct ViewerPresenterActivityTests {
  @Test("each kind of action has its symbol, and a problem has the warning symbol")
  func eachKindHasItsSymbol() {
    let lines: [(entry: ActivityLog.Entry, symbol: SymbolName)] = [
      (.init(id: "1", says: "clicked Save in TextEdit"), .click),
      (.init(id: "2", says: "typed in TextEdit"), .keyboard),
      (.init(id: "3", says: "scrolled in Opie"), .scroll),
      (.init(id: "4", says: "took a screenshot", operation: "screenshot"), .camera),
      (.init(id: "5", says: "opened Opie", operation: "run"), .window),
      (.init(id: "6", says: "ran a command", operation: "run"), .terminal),
      (.init(id: "7", says: "clicked in Opie", actor: .you), .person),
      (.init(id: "8", says: "gave back control", actor: .you, operation: "release"), .handRaised),
      (.init(id: "9", says: "couldn't click Save in TextEdit", outcome: .failed), .problem),
      (.init(id: "10", says: "couldn't type", outcome: .failed, actor: .you), .problem),
    ]
    for line in lines {
      #expect(ViewerPresenter.kind(line.entry).symbol == line.symbol, "\(line.entry.id)")
    }
    // A problem's tile is orange as well as its symbol a warning; the person's own is blue.
    #expect(ViewerPresenter.lead(lines[8].entry) == .failed)
    #expect(ViewerPresenter.lead(lines[6].entry) == .person)
    // No two kinds share a symbol, so the kind always reads.
    let symbols = Set(ViewerActivityKind.allCases.map(\.symbol))
    #expect(symbols.count == ViewerActivityKind.allCases.count)
  }

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
    sessions: [[String: Any]] = ViewerPresenterSessionsTests.three,
    mode: String = "agent",
    state: String = "working",
    pending: Int = 0
  ) throws -> ViewerSnapshot {
    var snapshot = try ViewerSnapshot.listing(sessions, mode: mode, state: state, pending: pending)
    snapshot.session.tab = .activity
    snapshot.panes.activity.log = ActivityLog(entries: log)
    snapshot.panes.activity.readAt = snapshot.date
    return snapshot
  }

  /// Six of Claude Code's rows, one of Codex's, the person's, and one no host named, newest first.
  static var mixedLog: [ActivityLog.Entry] {
    (0..<6)
      .map {
        ActivityLog.Entry(
          id: "claude-\($0)",
          says: "clicked Item \($0) in Opie",
          ageSeconds: Double($0),
          session: "3f9a1c07b2e4"
        )
      } + [
        ActivityLog.Entry(
          id: "codex",
          says: "opened Notes",
          ageSeconds: 10,
          operation: "open",
          session: "8be20c4d1a77"
        ),
        ActivityLog.Entry(id: "you", says: "clicked in Opie", actor: .you, ageSeconds: 20),
        ActivityLog.Entry(id: "terminal", says: "ran a command", ageSeconds: 30, operation: "run"),
      ]
  }

  @Test("All sessions lists each session's newest four lines with Show all")
  func allSessionsListsEachSessionsNewestFour() throws {
    let activity = ViewerPresenter.activity(try Self.onActivity(Self.mixedLog))
    #expect(activity.groups.isEmpty)
    #expect(activity.sessions.map(\.name) == ["Claude Code · understudy", "Codex · atlas-landing", "You", "Terminal"])
    let claude = try #require(activity.sessions.first)
    #expect(claude.lines.map(\.id) == ["claude-0", "claude-1", "claude-2", "claude-3"])
    #expect(claude.showAll?.title == "Show all 6")
    #expect(claude.showAll?.event == .chooseSession(.one("3f9a1c07b2e4")))
    #expect(claude.state == "Acting")
    #expect(activity.sessions[1].showAll == nil, "a session with four lines or fewer lists them all")
    // With one session live, the same log is grouped by time, as today.
    let alone = ViewerPresenter.activity(try Self.onActivity(Self.mixedLog, sessions: [SessionReplies.claudeActing]))
    #expect(alone.sessions.isEmpty)
    #expect(alone.groups.flatMap(\.lines).count == 9)
  }

  @Test("one session's pane offers All and Problems only")
  func oneSessionOffersAllAndProblemsOnly() throws {
    var snapshot = try Self.onActivity(Self.mixedLog)
    snapshot.session.chosenSession = .one("3f9a1c07b2e4")
    snapshot.panes.activity.filter = .you
    let activity = ViewerPresenter.activity(snapshot)
    #expect(activity.filters.tabs.map(\.id) == [.all, .problems])
    #expect(activity.filters.selected == .all, "a filter that means nothing inside one session reads as All")
    #expect(activity.title == "Claude Code · understudy")
    #expect(activity.titleIcon == SessionHost.claude.icon)
    #expect(activity.meta == "6 actions today")
    #expect(activity.sessions.isEmpty)
    let lines = activity.groups
      .flatMap(\.lines)
    #expect(lines.allSatisfy { $0.id.hasPrefix("claude-") })
  }

  @Test("only the causing session's uncertain line offers Recover")
  func onlyTheCausersUncertainLineOffersRecover() throws {
    let log = [
      ActivityLog.Entry(id: "c1", says: "clicked Show Center in Opie", outcome: .failed, session: "3f9a1c07b2e4"),
      ActivityLog.Entry(id: "x1", says: "couldn't click Save in Notes", outcome: .failed, session: "8be20c4d1a77"),
    ]
    let snapshot = try Self.onActivity(
      log,
      sessions: [SessionReplies.claudeNeedsYou, SessionReplies.codexWaiting(recovery: true)],
      mode: "recovering",
      state: "needsYou",
      pending: 1
    )
    let lines = ViewerPresenter.activity(snapshot)
      .sessions
      .flatMap(\.lines)
    let offering = lines.filter(\.offersRecovery)
      .map(\.id)
    #expect(offering == ["c1"])
    #expect(lines.first?.reason == "May not have finished. Every session waits until you recover.")
  }

  @Test("a refusal before anything was sent is no problem and keeps its quiet reason")
  func aRefusalIsNoProblem() throws {
    let refused = ActivityLog.Entry(
      id: "f1",
      says: "didn't type in Notes",
      outcome: .notSent,
      reason: "Another app was in front"
    )
    var snapshot = try Self.onActivity([refused], sessions: [])
    let line = try #require(ViewerPresenter.activity(snapshot).groups.first?.lines.first)
    #expect(line.lead == .notSent)
    #expect(line.kind == .keys)
    #expect(line.reason == "Another app was in front")
    snapshot.panes.activity.filter = .problems
    #expect(ViewerPresenter.activity(snapshot).groups.isEmpty)
  }
}
