import AtlasServiceTesting
import Foundation
import Testing

@testable import AtlasService

@Suite("ActivityMoments")
struct ActivityMomentsTests {
  /// The moments of `entries` in `scope`, read and looked at 10:31:42.
  ///
  /// - Parameters:
  ///   - entries: The log's rows, newest first.
  ///   - scope: Whose rows to form moments from.
  /// - Returns: The rules.
  private static func moments(_ entries: [ActivityLog.Entry], scope: ActivityScope = .all) -> ActivityMoments {
    ActivityMoments(ActivityFixtures.rules(entries), captures: [], scope: scope)
  }

  @Test("a screenshot reassigned to a new app leaves both split moments with independent folds")
  func aScreenshotSplitsWithoutSharingIdentity() throws {
    let click = ActivityFixtures.row(
      id: "textedit",
      says: "click textedit",
      ageSeconds: 30,
      session: "claude",
      verb: "click",
      app: "TextEdit"
    )
    let screenshot = ActivityFixtures.row(
      id: "screenshot",
      says: "screenshot screenshot",
      ageSeconds: 20,
      operation: "screenshot",
      session: "claude",
      verb: "screenshot",
      app: nil
    )
    let before = Self.moments([screenshot, click])
    let after = ActivityMoments(
      ActivityFixtures.rules([
        ActivityFixtures.row(
          id: "calculator",
          says: "click calculator",
          ageSeconds: 10,
          session: "claude",
          verb: "click",
          app: "Calculator"
        ), screenshot, click,
      ]),
      captures: [],
      scope: .all,
      preserving: before.moments
    )
    #expect(after.moments.map { $0.runs.map(\.id) } == [["calculator", "screenshot"], ["textedit"]])
    try Self.expectIndependentFolds(after, preserving: before.moments)
  }

  @Test("a command completing after a long gap leaves both split moments with independent folds")
  func aCompletedCommandSplitsWithoutSharingIdentity() throws {
    let click = ActivityFixtures.row(
      id: "click",
      says: "click click",
      ageSeconds: 600,
      session: "claude",
      verb: "click",
      app: "Opie"
    )
    var running = ActivityFixtures.row(
      id: "start",
      says: "run start",
      outcome: .running,
      ageSeconds: 590,
      operation: "run",
      session: "claude",
      verb: "run",
      app: nil
    )
    running.logicalId = "command"
    let before = Self.moments([running, click])
    var completed = ActivityFixtures.row(
      id: "end",
      says: "run end",
      ageSeconds: 10,
      operation: "run",
      session: "claude",
      verb: "run",
      app: nil
    )
    completed.logicalId = running.logicalId
    let after = ActivityMoments(
      ActivityFixtures.rules([completed, click]),
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
        now: ActivityFixtures.readAt
      )
      #expect(folds[ids[0]] == (opensFirst ? .open : .folded))
      #expect(folds[ids[1]] == (opensFirst ? .folded : .open))
    }
    let reread = ActivityMoments(
      ActivityFixtures.rules(after.moments.flatMap { $0.runs.map(\.entry) }),
      captures: [],
      scope: .all,
      preserving: after.moments
    )
    #expect(reread.moments.map(\.id) == ids)
  }

  @Test("a moment holding a running command keeps its identity and its fold choice when the command finishes")
  func aRunningCommandsMomentKeepsItsIdentityAndFoldChoice() throws {
    var running = ActivityFixtures.row(
      id: "start",
      says: "click start",
      outcome: .running,
      ageSeconds: 10,
      session: "claude",
      verb: "click",
      app: "Opie"
    )
    running.logicalId = "command"
    let before = Self.moments([
      running,
      ActivityFixtures.row(
        id: "older",
        says: "click older",
        ageSeconds: 20,
        session: "claude",
        verb: "click",
        app: "Opie"
      ),
    ])
    let id = try #require(before.moments.first?.id)
    let choices = [id: false]
    #expect(
      before.folds(
        of: before.moments,
        filter: .all,
        uncertainRow: nil,
        choices: choices,
        now: ActivityFixtures.readAt
      )[id] == .held
    )

    var completed = ActivityFixtures.row(
      id: "end",
      says: "click end",
      ageSeconds: 5,
      session: "claude",
      verb: "click",
      app: "Opie"
    )
    completed.logicalId = running.logicalId
    let after = ActivityMoments(
      ActivityFixtures.rules([completed]),
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
        now: ActivityFixtures.readAt
      )[id] == .folded
    )
    #expect(
      after.folds(
        of: after.moments,
        filter: .all,
        uncertainRow: nil,
        choices: [id: true],
        now: ActivityFixtures.readAt
      )[id] == .open
    )
  }

  @Test("a growing personal input run keeps its moment identity and fold choice")
  func aGrowingInputRunKeepsItsMoment() throws {
    var input = ActivityFixtures.row(
      id: "first",
      says: "click first",
      actor: .you,
      ageSeconds: 10,
      session: nil,
      verb: "click",
      app: "Opie"
    )
    input.logicalId = "2000000000"
    let before = Self.moments([input])
    let id = try #require(before.moments.first?.id)
    var grown = ActivityFixtures.row(
      id: "newest",
      says: "click newest",
      actor: .you,
      ageSeconds: 1,
      session: nil,
      verb: "click",
      app: "Opie"
    )
    grown.logicalId = input.logicalId
    let after = ActivityMoments(
      ActivityFixtures.rules([grown]),
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
        now: ActivityFixtures.readAt
      )[id] == .folded
    )
  }

  @Test("moment action totals include aggregated input while repeated rows keep their own count")
  func actionTotalsSurviveRowCollapsing() throws {
    let entries = [
      ActivityFixtures.row(id: "new", says: "sent input", actor: .you, ageSeconds: 1, count: 2107),
      ActivityFixtures.row(id: "old", says: "sent input", actor: .you, ageSeconds: 2, count: 12),
      ActivityFixtures.row(id: "plain", says: "clicked", actor: .you, ageSeconds: 3),
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
      ActivityFixtures.row(id: "c3", says: "type c3", ageSeconds: 10, session: "claude", verb: "type", app: "Opie"),
      ActivityFixtures.row(id: "c2", says: "key c2", ageSeconds: 40, session: "claude", verb: "key", app: "Opie"),
      ActivityFixtures.row(id: "c1", says: "click c1", ageSeconds: 90, session: "claude", verb: "click", app: "Opie"),
    ])
    .moments
    #expect(moments.map { $0.runs.map(\.id) } == [["c3", "c2", "c1"]])
    #expect(moments.first?.subject == .app(ActivityApp(name: "Opie")))
    #expect(moments.first?.actions == 3)
    #expect(moments.first?.startedAt == ActivityFixtures.readAt.addingTimeInterval(-90))
    #expect(moments.first?.endedAt == ActivityFixtures.readAt.addingTimeInterval(-10))
  }

  @Test("identical words in same-named apps with different bundle identities retain both moments")
  func identicalWordsCannotEraseBundleIdentity() {
    let entries = [
      ActivityFixtures.row(
        id: "one",
        says: "clicked in Opie",
        ageSeconds: 5,
        session: "claude",
        verb: "click",
        app: "Opie",
        bundleId: "com.one.opie"
      ),
      ActivityFixtures.row(
        id: "two",
        says: "clicked in Opie",
        ageSeconds: 10,
        session: "claude",
        verb: "click",
        app: "Opie",
        bundleId: "com.two.opie"
      ),
    ]
    let moments = Self.moments(entries).moments
    #expect(moments.count == 2)
    #expect(moments.map { $0.app?.bundleId } == ["com.one.opie", "com.two.opie"])
    #expect(moments.map(\.actions) == [1, 1])
  }

  @Test("a repeat with no bundle keeps the known bundle and cannot bridge two different apps")
  func repeatsRetainKnownIdentity() {
    let entries = [
      ActivityFixtures.row(
        id: "one",
        says: "clicked in Opie",
        ageSeconds: 5,
        session: "claude",
        verb: "click",
        app: "Opie",
        bundleId: "com.one.opie"
      ),
      ActivityFixtures.row(
        id: "unknown",
        says: "clicked in Opie",
        ageSeconds: 10,
        session: "claude",
        verb: "click",
        app: "Opie"
      ),
      ActivityFixtures.row(
        id: "two",
        says: "clicked in Opie",
        ageSeconds: 15,
        session: "claude",
        verb: "click",
        app: "Opie",
        bundleId: "com.two.opie"
      ),
    ]
    let moments = Self.moments(entries).moments
    #expect(moments.count == 2)
    #expect(moments.map { $0.app?.bundleId } == ["com.one.opie", "com.two.opie"])
    #expect(moments.map(\.actions) == [2, 1])
  }

  @Test("a gap of more than two minutes starts a new moment in the same app")
  func aGapOfMoreThanTwoMinutesStartsANewMoment() {
    let moments = Self.moments([
      ActivityFixtures.row(id: "c4", says: "click c4", ageSeconds: 10, session: "claude", verb: "click", app: "Opie"),
      ActivityFixtures.row(id: "c3", says: "type c3", ageSeconds: 130, session: "claude", verb: "type", app: "Opie"),
      ActivityFixtures.row(id: "c2", says: "key c2", ageSeconds: 251, session: "claude", verb: "key", app: "Opie"),
      ActivityFixtures.row(id: "c1", says: "click c1", ageSeconds: 300, session: "claude", verb: "click", app: "Opie"),
    ])
    .moments
    #expect(moments.map { $0.runs.map(\.id) } == [["c4", "c3"], ["c2", "c1"]], "120 s joins, 121 s does not")
  }

  @Test("identical actions more than two minutes apart stay in separate moments")
  func identicalActionsRespectMomentGaps() {
    let moments = Self.moments([
      ActivityFixtures.row(id: "new", says: "scrolled", ageSeconds: 10, session: "claude", verb: "scroll", app: "Opie"),
      ActivityFixtures.row(
        id: "old",
        says: "scrolled",
        ageSeconds: 610,
        session: "claude",
        verb: "scroll",
        app: "Opie"
      ),
    ])
    .moments
    #expect(moments.map { $0.runs.map(\.id) } == [["new"], ["old"]])
  }

  @Test("overlapping repeat runs retain their moment identity after their oldest row leaves")
  func overlappingRepeatsRetainTheirMomentIdentity() {
    let before = Self.moments([
      ActivityFixtures.row(id: "b", says: "scrolled", ageSeconds: 10, session: "claude", verb: "scroll", app: "Opie"),
      ActivityFixtures.row(id: "a", says: "scrolled", ageSeconds: 20, session: "claude", verb: "scroll", app: "Opie"),
    ])
    let after = ActivityMoments(
      ActivityFixtures.rules([
        ActivityFixtures.row(id: "c", says: "scrolled", ageSeconds: 5, session: "claude", verb: "scroll", app: "Opie"),
        ActivityFixtures.row(id: "b", says: "scrolled", ageSeconds: 10, session: "claude", verb: "scroll", app: "Opie"),
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
      ActivityFixtures.row(
        id: "n2",
        says: "click n2",
        ageSeconds: 5,
        session: "claude",
        verb: "click",
        app: "Notes",
        bundleId: "com.apple.Notes"
      ),
      ActivityFixtures.row(id: "n1", says: "click n1", ageSeconds: 10, session: "claude", verb: "click", app: "notes"),
      ActivityFixtures.row(
        id: "o1",
        says: "click o1",
        ageSeconds: 15,
        session: "claude",
        verb: "click",
        app: "Opie",
        bundleId: "com.insanearts.opie"
      ),
      ActivityFixtures.row(
        id: "x1",
        says: "click x1",
        ageSeconds: 20,
        session: "claude",
        verb: "click",
        app: "Opie",
        bundleId: "com.example.opie"
      ),
    ])
    .moments
    #expect(moments.map { $0.runs.map(\.id) } == [["n2", "n1"], ["o1"], ["x1"]], "two apps of one name are not one")
    #expect(moments.first?.app == ActivityApp(name: "Notes", bundleId: "com.apple.Notes"))
  }

  @Test("a screenshot between two clicks in Opie stays in Opie's moment")
  func aScreenshotBetweenTwoClicksStaysInOpiesMoment() {
    let moments = Self.moments([
      ActivityFixtures.row(id: "c2", says: "click c2", ageSeconds: 10, session: "claude", verb: "click", app: "Opie"),
      ActivityFixtures.row(
        id: "s1",
        says: "screenshot s1",
        ageSeconds: 20,
        operation: "screenshot",
        session: "claude",
        verb: "screenshot",
        app: nil
      ),
      ActivityFixtures.row(id: "c1", says: "click c1", ageSeconds: 30, session: "claude", verb: "click", app: "Opie"),
    ])
    .moments
    #expect(moments.map { $0.runs.map(\.id) } == [["c2", "s1", "c1"]])
  }

  @Test("a moment in no app takes the app of the first action with one, else reads as Commands or Screenshots")
  func aMomentInNoAppTakesTheFirstAppItMeets() {
    let joined = Self.moments([
      ActivityFixtures.row(
        id: "s1",
        says: "screenshot s1",
        ageSeconds: 5,
        operation: "screenshot",
        session: "claude",
        verb: "screenshot",
        app: nil
      ),
      ActivityFixtures.row(
        id: "r1",
        says: "run r1",
        ageSeconds: 10,
        operation: "run",
        session: "claude",
        verb: "run",
        app: nil
      ),
      ActivityFixtures.row(id: "c1", says: "click c1", ageSeconds: 20, session: "claude", verb: "click", app: "Notes"),
    ])
    .moments
    #expect(joined.map(\.subject) == [.app(ActivityApp(name: "Notes"))])
    let commands = Self.moments([
      ActivityFixtures.row(
        id: "s1",
        says: "screenshot s1",
        ageSeconds: 5,
        operation: "screenshot",
        session: "claude",
        verb: "screenshot",
        app: nil
      ),
      ActivityFixtures.row(
        id: "r1",
        says: "run r1",
        ageSeconds: 10,
        operation: "run",
        session: "claude",
        verb: "run",
        app: nil
      ),
    ])
    .moments
    #expect(commands.map(\.subject) == [.commands])
    let shots = Self.moments([
      ActivityFixtures.row(
        id: "s2",
        says: "screenshot s2",
        ageSeconds: 5,
        operation: "screenshot",
        session: "claude",
        verb: "screenshot",
        app: nil
      ),
      ActivityFixtures.row(
        id: "s1",
        says: "screenshot s1",
        ageSeconds: 10,
        operation: "screenshot",
        session: "claude",
        verb: "screenshot",
        app: nil
      ),
    ])
    .moments
    #expect(shots.map(\.subject) == [.screenshots])
  }

  @Test("two sessions working at once keep their own moments")
  func twoSessionsWorkingAtOnceKeepTheirOwnMoments() {
    let moments = Self.moments([
      ActivityFixtures.row(id: "a3", says: "click a3", ageSeconds: 5, session: "claude", verb: "click", app: "Opie"),
      ActivityFixtures.row(id: "b2", says: "click b2", ageSeconds: 10, session: "codex", verb: "click", app: "Notes"),
      ActivityFixtures.row(id: "a2", says: "type a2", ageSeconds: 15, session: "claude", verb: "type", app: "Opie"),
      ActivityFixtures.row(id: "b1", says: "type b1", ageSeconds: 20, session: "codex", verb: "type", app: "Notes"),
      ActivityFixtures.row(id: "a1", says: "click a1", ageSeconds: 25, session: "claude", verb: "click", app: "Opie"),
    ])
    .moments
    #expect(moments.map { $0.runs.map(\.id) } == [["a3", "a2", "a1"], ["b2", "b1"]])
    #expect(moments.map(\.stream) == [.session("claude"), .session("codex")])
  }

  @Test("the person's own actions form their own moments")
  func thePersonsOwnActionsFormTheirOwnMoments() {
    let moments = Self.moments([
      ActivityFixtures.row(id: "a2", says: "click a2", ageSeconds: 5, session: "claude", verb: "click", app: "Opie"),
      ActivityFixtures.row(
        id: "y1",
        says: "click y1",
        actor: .you,
        ageSeconds: 10,
        session: nil,
        verb: "click",
        app: "Opie"
      ),
      ActivityFixtures.row(id: "a1", says: "click a1", ageSeconds: 15, session: "claude", verb: "click", app: "Opie"),
    ])
    .moments
    #expect(moments.map { $0.runs.map(\.id) } == [["a2", "a1"], ["y1"]])
    #expect(moments.map(\.stream) == [.session("claude"), .you])
    let one = Self.moments(
      [
        ActivityFixtures.row(
          id: "y1",
          says: "click y1",
          actor: .you,
          ageSeconds: 10,
          session: nil,
          verb: "click",
          app: "Opie"
        ),
        ActivityFixtures.row(id: "a1", says: "click a1", ageSeconds: 15, session: "claude", verb: "click", app: "Opie"),
      ],
      scope: .session("claude")
    )
    #expect(one.moments.map(\.stream) == [.session("claude")], "a session's scope holds none of the person's own")
  }

  @Test("a moment keeps its id as newer actions join it")
  func aMomentKeepsItsIdAsNewerActionsJoinIt() {
    let older = [
      ActivityFixtures.row(id: "c2", says: "type c2", ageSeconds: 20, session: "claude", verb: "type", app: "Opie"),
      ActivityFixtures.row(id: "c1", says: "click c1", ageSeconds: 30, session: "claude", verb: "click", app: "Opie"),
    ]
    let before = Self.moments(older).moments
    let after = Self.moments(
      [
        ActivityFixtures.row(id: "c4", says: "key c4", ageSeconds: 2, session: "claude", verb: "key", app: "Opie"),
        ActivityFixtures.row(
          id: "c3",
          says: "screenshot c3",
          ageSeconds: 8,
          operation: "screenshot",
          session: "claude",
          verb: "screenshot",
          app: nil
        ),
      ] + older
    ).moments
    #expect(before.map(\.id) == ["session:claude|c1"])
    #expect(after.map(\.id) == before.map(\.id))
    #expect(after.first?.runs.map(\.id) == ["c4", "c3", "c2", "c1"])
  }

  @Test("a moment never spans two days")
  func aMomentNeverSpansTwoDays() {
    let midnight = ActivityFixtures.calendar
      .startOfDay(for: ActivityFixtures.readAt)
    let sinceMidnight = ActivityFixtures.readAt
      .timeIntervalSince(midnight)
    let moments = Self.moments([
      ActivityFixtures.row(
        id: "c2",
        says: "click c2",
        ageSeconds: sinceMidnight - 30,
        session: "claude",
        verb: "click",
        app: "Opie"
      ),
      ActivityFixtures.row(
        id: "c1",
        says: "click c1",
        ageSeconds: sinceMidnight + 30,
        session: "claude",
        verb: "click",
        app: "Opie"
      ),
    ])
    .moments
    #expect(moments.map { $0.runs.map(\.id) } == [["c2"], ["c1"]])
    #expect(moments.map(\.day) == [midnight, midnight.addingTimeInterval(-86_400)])
  }

  @Test("repeats are one line whatever stretch of time they cross, ending at their newest")
  func repeatsAreOneLine() {
    let moments = Self.moments([
      ActivityFixtures.row(id: "c3", says: "scrolled", ageSeconds: 100, session: "claude", verb: "scroll", app: "Opie"),
      ActivityFixtures.row(id: "c2", says: "scrolled", ageSeconds: 118, session: "claude", verb: "scroll", app: "Opie"),
      ActivityFixtures.row(id: "c1", says: "scrolled", ageSeconds: 125, session: "claude", verb: "scroll", app: "Opie"),
    ])
    .moments
    #expect(moments.first?.runs.map(\.id) == ["c1"])
    #expect(moments.first?.runs.first?.count == 3)
    #expect(moments.first?.endedAt == ActivityFixtures.readAt.addingTimeInterval(-100))
    #expect(moments.first?.actions == 3)
  }

  @Test("the newest moment and those ended in the last five minutes open by default")
  func theNewestAndTheRecentOpenByDefault() {
    let rules = Self.moments([
      ActivityFixtures.row(id: "e1", says: "click e1", ageSeconds: 10, session: "claude", verb: "click", app: "Notes"),
      ActivityFixtures.row(id: "d1", says: "click d1", ageSeconds: 60, session: "claude", verb: "click", app: "Mail"),
      ActivityFixtures.row(id: "c1", says: "click c1", ageSeconds: 290, session: "claude", verb: "click", app: "Maps"),
      ActivityFixtures.row(
        id: "b1",
        says: "click b1",
        ageSeconds: 299,
        session: "claude",
        verb: "click",
        app: "Safari"
      ),
      ActivityFixtures.row(id: "a1", says: "click a1", ageSeconds: 600, session: "claude", verb: "click", app: "Opie"),
    ])
    let shown = rules.shown(filter: .all, query: "")
    let folds = rules.folds(of: shown, filter: .all, uncertainRow: nil, choices: [:], now: ActivityFixtures.readAt)
    #expect(shown.map { folds[$0.id] } == [.open, .open, .open, .folded, .folded], "at most three")
    let old = Self.moments([
      ActivityFixtures.row(id: "b1", says: "click b1", ageSeconds: 900, session: "claude", verb: "click", app: "Notes"),
      ActivityFixtures.row(id: "a1", says: "click a1", ageSeconds: 1000, session: "claude", verb: "click", app: "Opie"),
    ])
    let oldShown = old.shown(filter: .all, query: "")
    let oldFolds = old.folds(
      of: oldShown,
      filter: .all,
      uncertainRow: nil,
      choices: [:],
      now: ActivityFixtures.readAt
    )
    #expect(oldShown.map { oldFolds[$0.id] } == [.open, .folded], "the newest opens however old")
  }

  @Test("the person's choice wins, and a moment under way or of unknown outcome is held open")
  func thePersonsChoiceWinsButHeldMomentsStayOpen() {
    let rules = Self.moments([
      ActivityFixtures.row(
        id: "c1",
        says: "click c1",
        outcome: .running,
        ageSeconds: 10,
        session: "claude",
        verb: "click",
        app: "Notes"
      ),
      ActivityFixtures.row(
        id: "b1",
        says: "click b1",
        outcome: .failed,
        ageSeconds: 900,
        session: "claude",
        verb: "click",
        app: "Mail"
      ),
      ActivityFixtures.row(id: "a1", says: "click a1", ageSeconds: 1000, session: "claude", verb: "click", app: "Opie"),
    ])
    let shown = rules.shown(filter: .all, query: "")
    let ids = shown.map(\.id)
    let choices = [ids[0]: false, ids[1]: false, ids[2]: true]
    let folds = rules.folds(
      of: shown,
      filter: .all,
      uncertainRow: "b1",
      choices: choices,
      now: ActivityFixtures.readAt
    )
    #expect(shown.map { folds[$0.id] } == [.held, .held, .open])
    let unchosen = rules.folds(
      of: shown,
      filter: .all,
      uncertainRow: nil,
      choices: [ids[0]: false],
      now: ActivityFixtures.readAt
    )
    #expect(shown.map { unchosen[$0.id] } == [.held, .folded, .folded])
    #expect(unchosen[ids[0]]?.isOpen == true && unchosen[ids[1]]?.isOpen == false)
  }

  @Test("Problems keeps only moments with a problem, held open, and keeps each moment's id")
  func problemsKeepsOnlyMomentsWithAProblem() {
    let rules = Self.moments([
      ActivityFixtures.row(id: "c2", says: "click c2", ageSeconds: 5, session: "claude", verb: "click", app: "Notes"),
      ActivityFixtures.row(
        id: "c1",
        says: "click c1",
        outcome: .failed,
        ageSeconds: 10,
        session: "claude",
        verb: "click",
        app: "Notes"
      ),
      ActivityFixtures.row(
        id: "b1",
        says: "click b1",
        outcome: .notSent,
        ageSeconds: 400,
        session: "claude",
        verb: "click",
        app: "Mail"
      ),
      ActivityFixtures.row(id: "a1", says: "click a1", ageSeconds: 900, session: "claude", verb: "click", app: "Opie"),
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
      now: ActivityFixtures.readAt
    )
    #expect(folds[all[0].id] == .held)
  }

  @Test("Screenshots and Typing keep their own steps")
  func screenshotsAndTypingKeepTheirOwnSteps() {
    let rules = Self.moments([
      ActivityFixtures.row(id: "k1", says: "key k1", ageSeconds: 5, session: "claude", verb: "key", app: "Opie"),
      ActivityFixtures.row(
        id: "s1",
        says: "screenshot s1",
        ageSeconds: 10,
        operation: "screenshot",
        session: "claude",
        verb: "screenshot",
        app: nil
      ),
      ActivityFixtures.row(id: "t1", says: "type t1", ageSeconds: 15, session: "claude", verb: "type", app: "Opie"),
      ActivityFixtures.row(id: "c1", says: "click c1", ageSeconds: 20, session: "claude", verb: "click", app: "Opie"),
    ])
    #expect(rules.shown(filter: .screenshots, query: "").map { $0.runs.map(\.id) } == [["s1"]])
    #expect(rules.shown(filter: .typing, query: "").map { $0.runs.map(\.id) } == [["k1", "t1"]])
  }

  @Test("the search matches a step's words, or the moment's app, which keeps every step")
  func theSearchMatchesWordsOrTheApp() {
    let rules = Self.moments([
      ActivityFixtures.row(
        id: "n2",
        says: "typed into Title in Notes",
        ageSeconds: 5,
        session: "claude",
        verb: "type",
        app: "Notes"
      ),
      ActivityFixtures.row(
        id: "n1",
        says: "clicked Save in Notes",
        ageSeconds: 10,
        session: "claude",
        verb: "click",
        app: "Notes"
      ),
      ActivityFixtures.row(
        id: "o1",
        says: "clicked Ring in Opie",
        ageSeconds: 400,
        session: "claude",
        verb: "click",
        app: "Opie"
      ),
    ])
    #expect(rules.shown(filter: .all, query: "save").map { $0.runs.map(\.id) } == [["n1"]])
    #expect(rules.shown(filter: .all, query: " opie ").map { $0.runs.map(\.id) } == [["o1"]])
    #expect(rules.shown(filter: .all, query: "notes").map { $0.runs.map(\.id) } == [["n2", "n1"]])
    #expect(rules.shown(filter: .typing, query: "notes").map { $0.runs.map(\.id) } == [["n2"]])
  }

  @Test("an open moment shows eight steps and counts the rest")
  func anOpenMomentShowsEightSteps() {
    let rows = (0..<11)
      .map {
        ActivityFixtures.row(
          id: "c\($0)",
          says: "\($0.isMultiple(of: 2) ? "click" : "type") c\($0)",
          ageSeconds: Double($0 * 5),
          session: "claude",
          verb: $0.isMultiple(of: 2) ? "click" : "type",
          app: "Opie"
        )
      }
    let moment = Self.moments(rows).moments[0]
    let folded = moment.steps(expanded: false)
    #expect(folded.shown.map(\.id) == (0..<8).map { "c\($0)" })
    #expect(folded.more == 3)
    #expect(moment.steps(expanded: true).shown.count == 11 && moment.steps(expanded: true).more == 0)
  }

  @Test("at most sixty moments are listed")
  func atMostSixtyMomentsAreListed() {
    let rows = (0..<70)
      .map {
        ActivityFixtures.row(
          id: "c\($0)",
          says: "click c\($0)",
          ageSeconds: Double($0 * 200),
          session: "claude",
          verb: "click",
          app: "Opie"
        )
      }
    #expect(Self.moments(rows).moments.count == 70)
    #expect(
      Self.moments(rows)
        .shown(filter: .all, query: "").count == ActivityMoments.mostMoments
    )
  }

  @Test("a problem says what the agent did next")
  func aProblemSaysWhatTheAgentDidNext() {
    let retried = Self.moments([
      ActivityFixtures.row(
        id: "c2",
        says: "click c2",
        ageSeconds: 10,
        session: "claude",
        verb: "click",
        app: "Notes",
        label: "Save"
      ),
      ActivityFixtures.row(id: "b1", says: "click b1", ageSeconds: 12, session: "codex", verb: "click", app: "Mail"),
      ActivityFixtures.row(
        id: "c1",
        says: "click c1",
        outcome: .failed,
        ageSeconds: 20,
        session: "claude",
        verb: "click",
        app: "Notes",
        label: "Save"
      ),
    ])
    let problem = retried.moments[0].runs[1]
    #expect(problem.id == "c1")
    #expect(retried.followUp(to: problem) == .workedOnRetry, "another session's action between is not what it did")
    let again = Self.moments([
      ActivityFixtures.row(
        id: "c2",
        says: "click c2",
        outcome: .failed,
        ageSeconds: 10,
        session: "claude",
        verb: "click",
        app: "Opie",
        label: "Save"
      ),
      ActivityFixtures.row(
        id: "c1",
        says: "click c1",
        outcome: .failed,
        ageSeconds: 20,
        session: "claude",
        verb: "click",
        app: "Opie",
        label: "Save"
      ),
    ])
    #expect(again.followUp(to: again.moments[0].runs[1]) == .failedAgain)
    let then = Self.moments([
      ActivityFixtures.row(id: "t1", says: "type t1", ageSeconds: 10, session: "claude", verb: "type", app: "Opie"),
      ActivityFixtures.row(
        id: "c1",
        says: "click c1",
        outcome: .notSent,
        ageSeconds: 20,
        session: "claude",
        verb: "click",
        app: "Opie"
      ),
    ])
    #expect(then.followUp(to: then.moments[0].runs[1]) == .then(then.moments[0].runs[0]))
    let underWay = Self.moments([
      ActivityFixtures.row(
        id: "c2",
        says: "click c2",
        outcome: .running,
        ageSeconds: 10,
        session: "claude",
        verb: "click",
        app: "Opie",
        label: "Save"
      ),
      ActivityFixtures.row(
        id: "c1",
        says: "click c1",
        outcome: .failed,
        ageSeconds: 20,
        session: "claude",
        verb: "click",
        app: "Opie",
        label: "Save"
      ),
    ])
    #expect(underWay.followUp(to: underWay.moments[0].runs[1]) == nil, "a retry under way says nothing yet")
    let late = Self.moments([
      ActivityFixtures.row(id: "c2", says: "click c2", ageSeconds: 10, session: "claude", verb: "click", app: "Opie"),
      ActivityFixtures.row(
        id: "c1",
        says: "click c1",
        outcome: .failed,
        ageSeconds: 131,
        session: "claude",
        verb: "click",
        app: "Opie"
      ),
    ])
    #expect(late.followUp(to: late.moments[1].runs[0]) == nil, "past two minutes nothing followed")
    let none = Self.moments([
      ActivityFixtures.row(
        id: "c1",
        says: "click c1",
        outcome: .failed,
        ageSeconds: 10,
        session: "claude",
        verb: "click",
        app: "Opie"
      )
    ])
    #expect(none.followUp(to: none.moments[0].runs[0]) == nil)
    let worked = Self.moments([
      ActivityFixtures.row(id: "c2", says: "click c2", ageSeconds: 10, session: "claude", verb: "click", app: "Opie"),
      ActivityFixtures.row(id: "c1", says: "click c1", ageSeconds: 20, session: "claude", verb: "click", app: "Opie"),
    ])
    #expect(worked.followUp(to: worked.moments[0].runs[1]) == nil, "an action that worked has no follow-up")
  }
}
