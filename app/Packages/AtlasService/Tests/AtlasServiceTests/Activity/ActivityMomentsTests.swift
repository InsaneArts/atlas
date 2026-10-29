import Foundation
import Testing

@testable import AtlasService

@Suite("ActivityMoments")
struct ActivityMomentsTests {
  /// A row `age` seconds old, read at 10:31:42 on 22 October 2026, in UTC.
  ///
  /// - Parameters:
  ///   - id: Its id.
  ///   - age: Its age, in seconds.
  ///   - verb: The verb Atlas wrote for it.
  ///   - app: The app it acted in; nil for none.
  ///   - bundleId: That app's bundle identifier.
  ///   - says: What it did; the verb and the id by default, so no two rows repeat each other.
  ///   - outcome: How it went.
  ///   - actor: Who did it.
  ///   - session: Its session's key; nil for none.
  ///   - operation: Its operation; a screenshot's for the screenshot verb, else input.
  /// - Returns: The row.
  static func row(
    _ id: String,
    _ age: Double,
    _ verb: String = "click",
    app: String? = "Opie",
    bundleId: String? = nil,
    says: String? = nil,
    outcome: ActivityLog.Outcome = .worked,
    actor: ActivityLog.Actor = .agent,
    session: String? = "claude",
    operation: String? = nil
  ) -> ActivityLog.Entry {
    ActivityLog.Entry(
      id: id,
      says: says ?? "\(verb) \(id)",
      outcome: outcome,
      actor: actor,
      ageSeconds: age,
      operation: operation ?? (verb == "screenshot" ? "screenshot" : "input"),
      session: session,
      verb: verb,
      app: app,
      bundleId: bundleId
    )
  }

  /// The moments of `entries` in `scope`, read and looked at 10:31:42.
  ///
  /// - Parameters:
  ///   - entries: The log's rows, newest first.
  ///   - scope: Whose rows to form moments from.
  /// - Returns: The rules.
  static func moments(_ entries: [ActivityLog.Entry], scope: ActivityScope = .all) -> ActivityMoments {
    ActivityMoments(ActivityGroupingTests.rules(entries), captures: [], scope: scope)
  }

  @Test("consecutive actions in one app form one moment")
  func consecutiveActionsInOneAppFormOneMoment() {
    let moments = Self.moments([
      Self.row("c3", 10, "type"),
      Self.row("c2", 40, "key"),
      Self.row("c1", 90),
    ])
    .moments
    #expect(moments.map { $0.runs.map(\.id) } == [["c3", "c2", "c1"]])
    #expect(moments.first?.subject == .app(ActivityApp(name: "Opie")))
    #expect(moments.first?.actions == 3)
    #expect(moments.first?.startedAt == ActivityGroupingTests.readAt.addingTimeInterval(-90))
    #expect(moments.first?.endedAt == ActivityGroupingTests.readAt.addingTimeInterval(-10))
  }

  @Test("a gap of more than two minutes starts a new moment in the same app")
  func aGapOfMoreThanTwoMinutesStartsANewMoment() {
    let moments = Self.moments([
      Self.row("c4", 10),
      Self.row("c3", 130, "type"),
      Self.row("c2", 251, "key"),
      Self.row("c1", 300),
    ])
    .moments
    #expect(moments.map { $0.runs.map(\.id) } == [["c4", "c3"], ["c2", "c1"]], "120 s joins, 121 s does not")
  }

  @Test("another app starts a new moment, and the same app by bundle id joins whatever its name's case")
  func anotherAppStartsANewMoment() {
    let moments = Self.moments([
      Self.row("n2", 5, app: "Notes", bundleId: "com.apple.Notes"),
      Self.row("n1", 10, app: "notes"),
      Self.row("o1", 15, app: "Opie", bundleId: "com.insanearts.opie"),
      Self.row("x1", 20, app: "Opie", bundleId: "com.example.opie"),
    ])
    .moments
    #expect(moments.map { $0.runs.map(\.id) } == [["n2", "n1"], ["o1"], ["x1"]], "two apps of one name are not one")
    #expect(moments.first?.app == ActivityApp(name: "Notes", bundleId: "com.apple.Notes"))
  }

  @Test("a screenshot between two clicks in Opie stays in Opie's moment")
  func aScreenshotBetweenTwoClicksStaysInOpiesMoment() {
    let moments = Self.moments([
      Self.row("c2", 10),
      Self.row("s1", 20, "screenshot", app: nil),
      Self.row("c1", 30),
    ])
    .moments
    #expect(moments.map { $0.runs.map(\.id) } == [["c2", "s1", "c1"]])
  }

  @Test("a moment in no app takes the app of the first action with one, else reads as Commands or Screenshots")
  func aMomentInNoAppTakesTheFirstAppItMeets() {
    let joined = Self.moments([
      Self.row("s1", 5, "screenshot", app: nil),
      Self.row("r1", 10, "run", app: nil, operation: "run"),
      Self.row("c1", 20, app: "Notes"),
    ])
    .moments
    #expect(joined.map(\.subject) == [.app(ActivityApp(name: "Notes"))])
    let commands = Self.moments([
      Self.row("s1", 5, "screenshot", app: nil),
      Self.row("r1", 10, "run", app: nil, operation: "run"),
    ])
    .moments
    #expect(commands.map(\.subject) == [.commands])
    let shots = Self.moments([
      Self.row("s2", 5, "screenshot", app: nil),
      Self.row("s1", 10, "screenshot", app: nil),
    ])
    .moments
    #expect(shots.map(\.subject) == [.screenshots])
  }

  @Test("two sessions working at once keep their own moments")
  func twoSessionsWorkingAtOnceKeepTheirOwnMoments() {
    let moments = Self.moments([
      Self.row("a3", 5, session: "claude"),
      Self.row("b2", 10, app: "Notes", session: "codex"),
      Self.row("a2", 15, "type", session: "claude"),
      Self.row("b1", 20, "type", app: "Notes", session: "codex"),
      Self.row("a1", 25, session: "claude"),
    ])
    .moments
    #expect(moments.map { $0.runs.map(\.id) } == [["a3", "a2", "a1"], ["b2", "b1"]])
    #expect(moments.map(\.stream) == [.session("claude"), .session("codex")])
  }

  @Test("the person's own actions form their own moments")
  func thePersonsOwnActionsFormTheirOwnMoments() {
    let moments = Self.moments([
      Self.row("a2", 5),
      Self.row("y1", 10, actor: .you, session: nil),
      Self.row("a1", 15),
    ])
    .moments
    #expect(moments.map { $0.runs.map(\.id) } == [["a2", "a1"], ["y1"]])
    #expect(moments.map(\.stream) == [.session("claude"), .you])
    let one = Self.moments(
      [Self.row("y1", 10, actor: .you, session: nil), Self.row("a1", 15)],
      scope: .session("claude")
    )
    #expect(one.moments.map(\.stream) == [.session("claude")], "a session's scope holds none of the person's own")
  }

  @Test("a moment keeps its id as newer actions join it")
  func aMomentKeepsItsIdAsNewerActionsJoinIt() {
    let older = [Self.row("c2", 20, "type"), Self.row("c1", 30)]
    let before = Self.moments(older).moments
    let after = Self.moments([Self.row("c4", 2, "key"), Self.row("c3", 8, "screenshot", app: nil)] + older).moments
    #expect(before.map(\.id) == ["session:claude|c1"])
    #expect(after.map(\.id) == before.map(\.id))
    #expect(after.first?.runs.map(\.id) == ["c4", "c3", "c2", "c1"])
  }

  @Test("a moment never spans two days")
  func aMomentNeverSpansTwoDays() {
    let midnight = ActivityGroupingTests.calendar
      .startOfDay(for: ActivityGroupingTests.readAt)
    let sinceMidnight = ActivityGroupingTests.readAt
      .timeIntervalSince(midnight)
    let moments = Self.moments([
      Self.row("c2", sinceMidnight - 30),
      Self.row("c1", sinceMidnight + 30),
    ])
    .moments
    #expect(moments.map { $0.runs.map(\.id) } == [["c2"], ["c1"]])
    #expect(moments.map(\.day) == [midnight, midnight.addingTimeInterval(-86_400)])
  }

  @Test("repeats are one line whatever stretch of time they cross, ending at their newest")
  func repeatsAreOneLine() {
    let moments = Self.moments([
      Self.row("c3", 100, "scroll", says: "scrolled"),
      Self.row("c2", 118, "scroll", says: "scrolled"),
      Self.row("c1", 125, "scroll", says: "scrolled"),
    ])
    .moments
    #expect(moments.first?.runs.map(\.id) == ["c1"])
    #expect(moments.first?.runs.first?.count == 3)
    #expect(moments.first?.endedAt == ActivityGroupingTests.readAt.addingTimeInterval(-100))
    #expect(moments.first?.actions == 3)
  }

  @Test("the newest moment and those ended in the last five minutes open by default")
  func theNewestAndTheRecentOpenByDefault() {
    let rules = Self.moments([
      Self.row("e1", 10, app: "Notes"),
      Self.row("d1", 60, app: "Mail"),
      Self.row("c1", 290, app: "Maps"),
      Self.row("b1", 299, app: "Safari"),
      Self.row("a1", 600, app: "Opie"),
    ])
    let shown = rules.shown(filter: .all, query: "")
    let folds = rules.folds(of: shown, filter: .all, uncertainRow: nil, choices: [:])
    #expect(shown.map { folds[$0.id] } == [.open, .open, .open, .folded, .folded], "at most three")
    let old = Self.moments([Self.row("b1", 900, app: "Notes"), Self.row("a1", 1000, app: "Opie")])
    let oldShown = old.shown(filter: .all, query: "")
    let oldFolds = old.folds(of: oldShown, filter: .all, uncertainRow: nil, choices: [:])
    #expect(oldShown.map { oldFolds[$0.id] } == [.open, .folded], "the newest opens however old")
  }

  @Test("the person's choice wins, and a moment under way or of unknown outcome is held open")
  func thePersonsChoiceWinsButHeldMomentsStayOpen() {
    let rules = Self.moments([
      Self.row("c1", 10, app: "Notes", outcome: .running),
      Self.row("b1", 900, app: "Mail", outcome: .failed),
      Self.row("a1", 1000, app: "Opie"),
    ])
    let shown = rules.shown(filter: .all, query: "")
    let ids = shown.map(\.id)
    let choices = [ids[0]: false, ids[1]: false, ids[2]: true]
    let folds = rules.folds(of: shown, filter: .all, uncertainRow: "b1", choices: choices)
    #expect(shown.map { folds[$0.id] } == [.held, .held, .open])
    let unchosen = rules.folds(of: shown, filter: .all, uncertainRow: nil, choices: [ids[0]: false])
    #expect(shown.map { unchosen[$0.id] } == [.held, .folded, .folded])
    #expect(unchosen[ids[0]]?.isOpen == true && unchosen[ids[1]]?.isOpen == false)
  }

  @Test("Problems keeps only moments with a problem, held open, and keeps each moment's id")
  func problemsKeepsOnlyMomentsWithAProblem() {
    let rules = Self.moments([
      Self.row("c2", 5, app: "Notes"),
      Self.row("c1", 10, app: "Notes", outcome: .failed),
      Self.row("b1", 400, app: "Mail", outcome: .notSent),
      Self.row("a1", 900, app: "Opie"),
    ])
    let all = rules.shown(filter: .all, query: "")
    let problems = rules.shown(filter: .problems, query: "")
    #expect(problems.map { $0.runs.map(\.id) } == [["c1"]], "a row not sent is no problem")
    #expect(problems.map(\.id) == [all[0].id])
    let folds = rules.folds(of: problems, filter: .problems, uncertainRow: nil, choices: [all[0].id: false])
    #expect(folds[all[0].id] == .held)
  }

  @Test("Screenshots and Typing keep their own steps")
  func screenshotsAndTypingKeepTheirOwnSteps() {
    let rules = Self.moments([
      Self.row("k1", 5, "key"),
      Self.row("s1", 10, "screenshot", app: nil),
      Self.row("t1", 15, "type"),
      Self.row("c1", 20),
    ])
    #expect(rules.shown(filter: .screenshots, query: "").map { $0.runs.map(\.id) } == [["s1"]])
    #expect(rules.shown(filter: .typing, query: "").map { $0.runs.map(\.id) } == [["k1", "t1"]])
  }

  @Test("the search matches a step's words, or the moment's app, which keeps every step")
  func theSearchMatchesWordsOrTheApp() {
    let rules = Self.moments([
      Self.row("n2", 5, "type", app: "Notes", says: "typed into Title in Notes"),
      Self.row("n1", 10, app: "Notes", says: "clicked Save in Notes"),
      Self.row("o1", 400, app: "Opie", says: "clicked Ring in Opie"),
    ])
    #expect(rules.shown(filter: .all, query: "save").map { $0.runs.map(\.id) } == [["n1"]])
    #expect(rules.shown(filter: .all, query: " opie ").map { $0.runs.map(\.id) } == [["o1"]])
    #expect(rules.shown(filter: .all, query: "notes").map { $0.runs.map(\.id) } == [["n2", "n1"]])
    #expect(rules.shown(filter: .typing, query: "notes").map { $0.runs.map(\.id) } == [["n2"]])
  }

  @Test("an open moment shows eight steps and counts the rest")
  func anOpenMomentShowsEightSteps() {
    let rows = (0..<11)
      .map { Self.row("c\($0)", Double($0 * 5), $0.isMultiple(of: 2) ? "click" : "type") }
    let moment = Self.moments(rows).moments[0]
    let folded = moment.steps(expanded: false)
    #expect(folded.shown.map(\.id) == (0..<8).map { "c\($0)" })
    #expect(folded.more == 3)
    #expect(moment.steps(expanded: true).shown.count == 11 && moment.steps(expanded: true).more == 0)
  }

  @Test("at most sixty moments are listed")
  func atMostSixtyMomentsAreListed() {
    let rows = (0..<70)
      .map { Self.row("c\($0)", Double($0 * 200)) }
    #expect(Self.moments(rows).moments.count == 70)
    #expect(Self.moments(rows).shown(filter: .all, query: "").count == ActivityMoments.mostMoments)
  }

  @Test("a problem says what the agent did next")
  func aProblemSaysWhatTheAgentDidNext() {
    let retried = Self.moments([
      Self.row("c2", 10, app: "Notes"),
      Self.row("b1", 12, app: "Mail", session: "codex"),
      Self.row("c1", 20, app: "Notes", outcome: .failed),
    ])
    let problem = retried.moments[0].runs[1]
    #expect(problem.id == "c1")
    #expect(retried.followUp(to: problem) == .workedOnRetry, "another session's action between is not what it did")
    let again = Self.moments([Self.row("c2", 10, outcome: .failed), Self.row("c1", 20, outcome: .failed)])
    #expect(again.followUp(to: again.moments[0].runs[1]) == .failedAgain)
    let then = Self.moments([Self.row("t1", 10, "type"), Self.row("c1", 20, outcome: .notSent)])
    #expect(then.followUp(to: then.moments[0].runs[1]) == .then(then.moments[0].runs[0]))
    let underWay = Self.moments([Self.row("c2", 10, outcome: .running), Self.row("c1", 20, outcome: .failed)])
    #expect(underWay.followUp(to: underWay.moments[0].runs[1]) == nil, "a retry under way says nothing yet")
    let late = Self.moments([Self.row("c2", 10), Self.row("c1", 131, outcome: .failed)])
    #expect(late.followUp(to: late.moments[1].runs[0]) == nil, "past two minutes nothing followed")
    let none = Self.moments([Self.row("c1", 10, outcome: .failed)])
    #expect(none.followUp(to: none.moments[0].runs[0]) == nil)
    let worked = Self.moments([Self.row("c2", 10), Self.row("c1", 20)])
    #expect(worked.followUp(to: worked.moments[0].runs[1]) == nil, "an action that worked has no follow-up")
  }
}
