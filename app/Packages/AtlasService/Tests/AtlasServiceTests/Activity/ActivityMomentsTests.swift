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
  ///   - label: The recorded target, when it identifies the action.
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
    label: String? = nil,
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
      label: label,
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

  @Test("a screenshot reassigned to a new app leaves both split moments with independent folds")
  func aScreenshotSplitsWithoutSharingIdentity() throws {
    let click = Self.row("textedit", 30, app: "TextEdit")
    let screenshot = Self.row("screenshot", 20, "screenshot", app: nil)
    let before = Self.moments([screenshot, click])
    let after = ActivityMoments(
      ActivityGroupingTests.rules([Self.row("calculator", 10, app: "Calculator"), screenshot, click]),
      captures: [],
      scope: .all,
      preserving: before.moments
    )
    #expect(after.moments.map { $0.runs.map(\.id) } == [["calculator", "screenshot"], ["textedit"]])
    try Self.expectIndependentFolds(after, preserving: before.moments)
  }

  @Test("a command completing after a long gap leaves both split moments with independent folds")
  func aCompletedCommandSplitsWithoutSharingIdentity() throws {
    let click = Self.row("click", 600)
    var running = Self.row("start", 590, "run", app: nil, outcome: .running, operation: "run")
    running.logicalId = "command"
    let before = Self.moments([running, click])
    var completed = Self.row("end", 10, "run", app: nil, operation: "run")
    completed.logicalId = running.logicalId
    let after = ActivityMoments(
      ActivityGroupingTests.rules([completed, click]),
      captures: [],
      scope: .all,
      preserving: before.moments
    )
    #expect(after.moments.map { $0.runs.map(\.id) } == [["end"], ["click"]])
    try Self.expectIndependentFolds(after, preserving: before.moments)
  }

  /// Both split fragments can be folded independently, and repeating the same reading keeps their identities.
  private static func expectIndependentFolds(_ after: ActivityMoments, preserving before: [ActivityMoment]) throws {
    try #require(after.moments.count == 2)
    let ids = after.moments.map(\.id)
    try #require(Set(ids).count == 2)
    #expect(ids[0] == before.first?.id)
    for opensFirst in [true, false] {
      let folds = after.folds(
        of: after.moments,
        filter: .all,
        uncertainRow: nil,
        choices: [ids[0]: opensFirst, ids[1]: !opensFirst],
        now: ActivityGroupingTests.readAt
      )
      #expect(folds[ids[0]] == (opensFirst ? .open : .folded))
      #expect(folds[ids[1]] == (opensFirst ? .folded : .open))
    }
    let reread = ActivityMoments(
      ActivityGroupingTests.rules(after.moments.flatMap { $0.runs.map(\.entry) }),
      captures: [],
      scope: .all,
      preserving: after.moments
    )
    #expect(reread.moments.map(\.id) == ids)
  }

  @Test("a moment holding a running command keeps its identity and its fold choice when the command finishes")
  func aRunningCommandsMomentKeepsItsIdentityAndFoldChoice() throws {
    var running = Self.row("start", 10, outcome: .running)
    running.logicalId = "command"
    let before = Self.moments([running, Self.row("older", 20)])
    let id = try #require(before.moments.first?.id)
    let choices = [id: false]
    #expect(
      before.folds(
        of: before.moments,
        filter: .all,
        uncertainRow: nil,
        choices: choices,
        now: ActivityGroupingTests.readAt
      )[id] == .held
    )

    var completed = Self.row("end", 5)
    completed.logicalId = running.logicalId
    let after = ActivityMoments(
      ActivityGroupingTests.rules([completed]),
      captures: [],
      scope: .all,
      preserving: before.moments
    )
    // No row id overlaps, and the oldest row that seeded the moment has left.
    #expect(after.moments.map(\.id) == [id])
    #expect(after.moments.first?.runs.first?.id == "end")
    #expect(
      after.folds(
        of: after.moments,
        filter: .all,
        uncertainRow: nil,
        choices: choices,
        now: ActivityGroupingTests.readAt
      )[id] == .folded
    )
    #expect(
      after.folds(
        of: after.moments,
        filter: .all,
        uncertainRow: nil,
        choices: [id: true],
        now: ActivityGroupingTests.readAt
      )[id] == .open
    )
  }

  @Test("a growing personal input run keeps its moment identity and fold choice")
  func aGrowingInputRunKeepsItsMoment() throws {
    var input = Self.row("first", 10, actor: .you, session: nil)
    input.logicalId = "2000000000"
    let before = Self.moments([input])
    let id = try #require(before.moments.first?.id)
    var grown = Self.row("newest", 1, actor: .you, session: nil)
    grown.logicalId = input.logicalId
    let after = ActivityMoments(
      ActivityGroupingTests.rules([grown]),
      captures: [],
      scope: .all,
      preserving: before.moments
    )
    #expect(after.moments.map(\.id) == [id])
    #expect(after.moments.first?.runs.first?.id == "newest")
    #expect(
      after.folds(
        of: after.moments,
        filter: .all,
        uncertainRow: nil,
        choices: [id: false],
        now: ActivityGroupingTests.readAt
      )[id] == .folded
    )
  }

  @Test("moment action totals include aggregated input while repeated rows keep their own count")
  func actionTotalsSurviveRowCollapsing() throws {
    let entries = [
      ActivityLog.Entry(id: "new", says: "sent input", actor: .you, ageSeconds: 1, count: 2107),
      ActivityLog.Entry(id: "old", says: "sent input", actor: .you, ageSeconds: 2, count: 12),
      ActivityLog.Entry(id: "plain", says: "clicked", actor: .you, ageSeconds: 3),
    ]
    let moment = try #require(Self.moments(entries).moments.first)
    #expect(moment.runs.count == 2)
    #expect(moment.runs.first?.count == 2)
    #expect(moment.runs.first?.actions == 2119)
    #expect(moment.actions == 2120)
    #expect(Self.moments([entries[0]]).moments.first?.actions == 2107)
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

  @Test("identical words in same-named apps with different bundle identities retain both moments")
  func identicalWordsCannotEraseBundleIdentity() {
    let entries = [
      Self.row("one", 5, app: "Opie", bundleId: "com.one.opie", says: "clicked in Opie"),
      Self.row("two", 10, app: "Opie", bundleId: "com.two.opie", says: "clicked in Opie"),
    ]
    let moments = Self.moments(entries).moments
    #expect(moments.count == 2)
    #expect(moments.map { $0.app?.bundleId } == ["com.one.opie", "com.two.opie"])
    #expect(moments.map(\.actions) == [1, 1])
  }

  @Test("a repeat with no bundle keeps the known bundle and cannot bridge two different apps")
  func repeatsRetainKnownIdentity() {
    let entries = [
      Self.row("one", 5, app: "Opie", bundleId: "com.one.opie", says: "clicked in Opie"),
      Self.row("unknown", 10, app: "Opie", says: "clicked in Opie"),
      Self.row("two", 15, app: "Opie", bundleId: "com.two.opie", says: "clicked in Opie"),
    ]
    let moments = Self.moments(entries).moments
    #expect(moments.count == 2)
    #expect(moments.map { $0.app?.bundleId } == ["com.one.opie", "com.two.opie"])
    #expect(moments.map(\.actions) == [2, 1])
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

  @Test("identical actions more than two minutes apart stay in separate moments")
  func identicalActionsRespectMomentGaps() {
    let moments = Self.moments([
      Self.row("new", 10, "scroll", says: "scrolled"),
      Self.row("old", 610, "scroll", says: "scrolled"),
    ])
    .moments
    #expect(moments.map { $0.runs.map(\.id) } == [["new"], ["old"]])
  }

  @Test("overlapping repeat runs retain their moment identity after their oldest row leaves")
  func overlappingRepeatsRetainTheirMomentIdentity() {
    let before = Self.moments([
      Self.row("b", 10, "scroll", says: "scrolled"),
      Self.row("a", 20, "scroll", says: "scrolled"),
    ])
    let after = ActivityMoments(
      ActivityGroupingTests.rules([
        Self.row("c", 5, "scroll", says: "scrolled"),
        Self.row("b", 10, "scroll", says: "scrolled"),
      ]),
      captures: [],
      scope: .all,
      preserving: before.moments
    )
    #expect(after.moments.map(\.id) == before.moments.map(\.id))
    #expect(after.moments.first?.runs.first?.id == "b")
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
    let folds = rules.folds(of: shown, filter: .all, uncertainRow: nil, choices: [:], now: ActivityGroupingTests.readAt)
    #expect(shown.map { folds[$0.id] } == [.open, .open, .open, .folded, .folded], "at most three")
    let old = Self.moments([Self.row("b1", 900, app: "Notes"), Self.row("a1", 1000, app: "Opie")])
    let oldShown = old.shown(filter: .all, query: "")
    let oldFolds = old.folds(
      of: oldShown,
      filter: .all,
      uncertainRow: nil,
      choices: [:],
      now: ActivityGroupingTests.readAt
    )
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
    let folds = rules.folds(
      of: shown,
      filter: .all,
      uncertainRow: "b1",
      choices: choices,
      now: ActivityGroupingTests.readAt
    )
    #expect(shown.map { folds[$0.id] } == [.held, .held, .open])
    let unchosen = rules.folds(
      of: shown,
      filter: .all,
      uncertainRow: nil,
      choices: [ids[0]: false],
      now: ActivityGroupingTests.readAt
    )
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
    let folds = rules.folds(
      of: problems,
      filter: .problems,
      uncertainRow: nil,
      choices: [all[0].id: false],
      now: ActivityGroupingTests.readAt
    )
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
      Self.row("c2", 10, app: "Notes", label: "Save"),
      Self.row("b1", 12, app: "Mail", session: "codex"),
      Self.row("c1", 20, app: "Notes", outcome: .failed, label: "Save"),
    ])
    let problem = retried.moments[0].runs[1]
    #expect(problem.id == "c1")
    #expect(retried.followUp(to: problem) == .workedOnRetry, "another session's action between is not what it did")
    let again = Self.moments([
      Self.row("c2", 10, outcome: .failed, label: "Save"), Self.row("c1", 20, outcome: .failed, label: "Save"),
    ])
    #expect(again.followUp(to: again.moments[0].runs[1]) == .failedAgain)
    let then = Self.moments([Self.row("t1", 10, "type"), Self.row("c1", 20, outcome: .notSent)])
    #expect(then.followUp(to: then.moments[0].runs[1]) == .then(then.moments[0].runs[0]))
    let underWay = Self.moments([
      Self.row("c2", 10, outcome: .running, label: "Save"), Self.row("c1", 20, outcome: .failed, label: "Save"),
    ])
    #expect(underWay.followUp(to: underWay.moments[0].runs[1]) == nil, "a retry under way says nothing yet")
    let late = Self.moments([Self.row("c2", 10), Self.row("c1", 131, outcome: .failed)])
    #expect(late.followUp(to: late.moments[1].runs[0]) == nil, "past two minutes nothing followed")
    let none = Self.moments([Self.row("c1", 10, outcome: .failed)])
    #expect(none.followUp(to: none.moments[0].runs[0]) == nil)
    let worked = Self.moments([Self.row("c2", 10), Self.row("c1", 20)])
    #expect(worked.followUp(to: worked.moments[0].runs[1]) == nil, "an action that worked has no follow-up")
  }
}
