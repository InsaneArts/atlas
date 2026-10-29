import AtlasService
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("ViewerPresenter+Activity")
struct ViewerPresenterActivityTests {
  @Test("each kind of action has its glyph, and a problem has the warning glyph")
  func eachKindHasItsGlyph() {
    let rows: [(entry: ActivityLog.Entry, kind: ViewerActivityKind)] = [
      (.init(id: "1", says: "clicked Save in TextEdit", verb: "click"), .pointer),
      (.init(id: "2", says: "typed in TextEdit", verb: "type"), .typing),
      (.init(id: "3", says: "pressed ⌘S in TextEdit", verb: "key"), .keys),
      (.init(id: "4", says: "scrolled in Opie"), .scroll),
      (.init(id: "5", says: "took a screenshot", operation: "screenshot"), .screenshot),
      (.init(id: "6", says: "opened Opie", operation: "open"), .window),
      (.init(id: "7", says: "ran a command", operation: "run"), .command),
      (.init(id: "8", says: "clicked in Opie", actor: .you), .person),
      (.init(id: "9", says: "gave back control", actor: .you, operation: "release"), .control),
      (.init(id: "10", says: "couldn't click Save in TextEdit", outcome: .failed), .problem),
    ]
    for row in rows {
      #expect(ViewerPresenter.stepKind(row.entry) == row.kind, "\(row.entry.id)")
    }
    // No two kinds share a glyph, so the kind always reads.
    let symbols = Set(ViewerActivityKind.allCases.map(\.symbol))
    #expect(symbols.count == ViewerActivityKind.allCases.count)
  }

  @Test("consecutive actions in one app form one moment under its icon, its steps never naming the app")
  func actionsInOneAppFormOneMoment() throws {
    let activity = ViewerPresenter.activity(try Self.onActivity(Self.textEditLog, sessions: []))
    let moments = Self.moments(activity)
    #expect(moments.map(\.title) == ["TextEdit"])
    #expect(moments.first?.isOpen == true)
    #expect(moments.first?.icon == .app(.monogram(AppMonogram(name: "TextEdit"))))
    let words = Self.steps(activity)
      .map(\.words.text)
    #expect(words == ["Pressed ⌘S", "Typed into Name", "Clicked Save", "Opened TextEdit"])
  }

  @Test("a key press shows its keys as caps")
  func aKeyPressShowsItsKeysAsCaps() throws {
    let steps = Self.steps(ViewerPresenter.activity(try Self.onActivity(Self.textEditLog, sessions: [])))
    let pressed = try #require(steps.first)
    #expect(pressed.words.verb == "Pressed")
    #expect(pressed.words.keys == [["⌘", "S"]])
    #expect(pressed.words.target.isEmpty, "the chord is drawn as caps, never as words too")
    #expect(pressed.kind == .keys)
  }

  @Test("a typed action names its field only when the row names one")
  func aTypedActionNamesItsFieldOnlyWhenTheRowDoes() throws {
    let steps = Self.steps(ViewerPresenter.activity(try Self.onActivity(Self.textEditLog, sessions: [])))
    #expect(steps[1].words == ViewerStepWords("Typed into", target: "Name"))
    let plain = Self.steps(
      ViewerPresenter.activity(
        try Self.onActivity(
          [Self.row("t1", "typed in TextEdit", verb: "type", age: 0)],
          sessions: []
        )
      )
    )
    #expect(plain.first?.words == ViewerStepWords("Typed"))
  }

  @Test("a command shows no arguments")
  func aCommandShowsNoArguments() throws {
    let log = [
      ActivityLog.Entry(id: "r2", says: "ran swift build --verbose", outcome: .failed, ageSeconds: 1, operation: "run"),
      ActivityLog.Entry(id: "r1", says: "ran rm -rf build", ageSeconds: 5, operation: "run"),
    ]
    let activity = ViewerPresenter.activity(try Self.onActivity(log, sessions: []))
    #expect(Self.moments(activity).map(\.title) == ["Commands"])
    #expect(Self.steps(activity).map(\.words.text) == ["A command failed", "Ran a command"])
  }

  @Test("a moment's header never shows a window's title")
  func aMomentsHeaderNeverShowsAWindowsTitle() throws {
    var snapshot = try Self.onActivity(Self.textEditLog, sessions: [])
    snapshot.panes.apps.apps = [
      HelperApp(
        pid: 7,
        name: "TextEdit",
        front: true,
        windows: [HelperWindow(id: 1, title: "Secret plan.rtf", width: 800, height: 600)]
      )
    ]
    let activity = ViewerPresenter.activity(snapshot)
    let shown = String(describing: activity)
    #expect(!shown.contains("Secret plan"), "no window's title reaches Activity")
  }

  @Test("a folded moment says its app, its count and its span")
  func aFoldedMomentSaysItsAppCountAndSpan() throws {
    let older = (0..<9)
      .reversed()
      .map { Self.row("e\($0)", "clicked in TextEdit", verb: "click", age: Double(600 - $0 * 20)) }
    let log = [Self.row("n1", "opened Notes", verb: "open", app: "Notes", age: 2)] + older
    let moments = Self.moments(ViewerPresenter.activity(try Self.onActivity(log, sessions: [])))
    let folded = try #require(moments.last)
    #expect(folded.title == "TextEdit")
    #expect(!folded.isOpen)
    #expect(folded.span == "9 actions · 10:21–10:24")
    #expect(folded.age == "7 min")
  }

  @Test("opening a folded moment keeps it open as new actions arrive")
  func openingAFoldedMomentKeepsItOpen() throws {
    let older = (0..<3)
      .map { Self.row("e\($0)", "clicked in TextEdit", verb: "click", age: Double(600 + $0 * 10)) }
    var snapshot = try Self.onActivity(older, sessions: [])
    var moment = try #require(Self.moments(ViewerPresenter.activity(snapshot)).first)
    snapshot.panes.activity.choices[moment.id] = false
    #expect(Self.moments(ViewerPresenter.activity(snapshot)).first?.isOpen == false)
    snapshot.panes.activity.choices[moment.id] = true
    let newer = Self.row("e9", "clicked in TextEdit", verb: "click", age: 590)
    snapshot.panes.activity.log = ActivityLog(
      entries: [Self.row("n1", "opened Notes", verb: "open", app: "Notes", age: 0), newer] + older
    )
    moment = try #require(Self.moments(ViewerPresenter.activity(snapshot)).last)
    #expect(moment.title == "TextEdit")
    #expect(moment.isOpen, "the person's choice outlasts newer actions")
  }

  @Test("the filters count the problems and screenshots there are")
  func theFiltersCountWhatWasDone() throws {
    let log =
      Self.textEditLog + [
        ActivityLog.Entry(id: "s1", says: "took a screenshot", ageSeconds: 40, operation: "screenshot"),
        Self.row("f1", "couldn't click Save in TextEdit", verb: "click", age: 50, outcome: .failed),
      ]
    let activity = ViewerPresenter.activity(try Self.onActivity(log, sessions: []))
    #expect(activity.filters.tabs.map(\.count) == [nil, 1, 1, nil])
  }

  @Test("the Activity search follows the sidebar's query while Activity shows")
  func theActivitySearchFollowsTheSidebar() throws {
    var snapshot = try Self.onActivity(Self.mixedLog, sessions: [SessionReplies.claudeActing])
    snapshot.session.search.query = "notes"
    #expect(Self.moments(ViewerPresenter.activity(snapshot)).map(\.title) == ["Notes"])
    snapshot.session.search.query = "nothing like it"
    let none = ViewerPresenter.activity(snapshot)
    #expect(none.items.isEmpty)
    #expect(none.empty?.headline == ViewerWords.noActivityMatch("nothing like it"))
    #expect(none.showsFilters, "a search that finds nothing keeps the filters")
  }

  @Test("one session's pane lists only its moments, under its name")
  func oneSessionListsOnlyItsMoments() throws {
    var snapshot = try Self.onActivity(Self.mixedLog)
    snapshot.session.chosenSession = .one("3f9a1c07b2e4")
    let activity = ViewerPresenter.activity(snapshot)
    #expect(activity.filters.tabs.map(\.id) == ViewerActivityFilter.allCases)
    #expect(activity.title == "Claude Code · understudy")
    #expect(activity.meta == "6 actions today")
    #expect(Self.moments(activity).map(\.title) == ["Opie"])
    #expect(Self.moments(activity).allSatisfy { $0.session == nil }, "one session's moments need no name")
  }

  @Test("only the causing session's uncertain step offers Recover, and its moment stays open")
  func onlyTheCausersUncertainStepOffersRecover() throws {
    let log = [
      Self.row(
        "c1",
        "clicked Show Center in Opie",
        verb: "click",
        app: "Opie",
        age: 0,
        outcome: .failed,
        session: Self.claude
      ),
      Self.row(
        "x1",
        "couldn't click Save in Notes",
        verb: "click",
        app: "Notes",
        age: 1,
        outcome: .failed,
        session: Self.codex
      ),
    ]
    var snapshot = try Self.onActivity(
      log,
      sessions: [SessionReplies.claudeNeedsYou, SessionReplies.codexWaiting(recovery: true)],
      mode: "recovering",
      state: "needsYou",
      pending: 1
    )
    snapshot.panes.activity.choices = Dictionary(
      uniqueKeysWithValues: Self.moments(ViewerPresenter.activity(snapshot)).map { ($0.id, false) }
    )
    let activity = ViewerPresenter.activity(snapshot)
    let offering = Self.steps(activity)
      .filter(\.offersRecovery)
      .map(\.id)
    #expect(offering == ["c1"])
    #expect(Self.steps(activity).first?.problem?.why == "May not have finished. Every session waits until you recover.")
    #expect(Self.moments(activity).first?.canFold == false, "the moment holding it offers no fold")
  }

  @Test("a refusal before anything was sent is no problem, and says what the agent did next")
  func aRefusalIsNoProblem() throws {
    let log = [
      Self.row("f2", "typed in Notes", verb: "type", app: "Notes", age: 1),
      Self.row(
        "f1",
        "didn't type in Notes",
        verb: "type",
        app: "Notes",
        age: 3,
        outcome: .notSent,
        reason: "Another app was in front"
      ),
    ]
    var snapshot = try Self.onActivity(log, sessions: [])
    let step = try #require(Self.steps(ViewerPresenter.activity(snapshot)).last)
    #expect(step.mark == .notSent)
    #expect(step.kind == .typing)
    #expect(
      step.problem
        == ViewerActivityProblem(why: "Another app was in front.", next: "The agent tried again, and it worked.")
    )
    snapshot.panes.activity.filter = .problems
    #expect(ViewerPresenter.activity(snapshot).items.isEmpty)
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
    snapshot.session.place = .activity
    snapshot.date = Date(timeIntervalSince1970: 1_792_665_102)
    snapshot.timeZone = TimeZone(secondsFromGMT: 0) ?? .current
    snapshot.locale = Locale(identifier: "de_DE")
    snapshot.panes.activity.log = ActivityLog(entries: log)
    snapshot.panes.activity.readAt = snapshot.date
    return snapshot
  }

  /// Claude Code's session key.
  static let claude = "3f9a1c07b2e4"
  /// Codex's session key.
  static let codex = "8be20c4d1a77"

  /// A row an agent wrote in an app.
  ///
  /// - Parameters:
  ///   - id: The row's id.
  ///   - says: What it did, in the service's words.
  ///   - verb: The verb Atlas wrote for it.
  ///   - app: The app's name; TextEdit by default.
  ///   - label: The control or field it named, or the keys; none by default.
  ///   - keys: The keys as caps; none by default.
  ///   - age: How long before the reading it started.
  ///   - outcome: How it went; worked by default.
  ///   - reason: Why it did not work; none by default.
  ///   - session: The host session's key; none by default.
  /// - Returns: The row.
  static func row(
    _ id: String,
    _ says: String,
    verb: String,
    app: String = "TextEdit",
    label: String? = nil,
    keys: [[String]] = [],
    age: Double,
    outcome: ActivityLog.Outcome = .worked,
    reason: String? = nil,
    session: String? = nil
  ) -> ActivityLog.Entry {
    ActivityLog.Entry(
      id: id,
      says: says,
      outcome: outcome,
      ageSeconds: age,
      reason: reason,
      operation: verb == "open" ? "open" : "input",
      session: session,
      verb: verb,
      app: app,
      label: label,
      keys: keys
    )
  }

  /// An agent's four steps in TextEdit, newest first: ⌘S pressed, Name typed into, Save clicked, TextEdit opened.
  static let textEditLog = [
    row("k1", "pressed ⌘S in TextEdit", verb: "key", label: "⌘S", keys: [["⌘", "S"]], age: 0),
    row("t1", "typed into Name in TextEdit", verb: "type", label: "Name", age: 5),
    row("c1", "clicked Save in TextEdit", verb: "click", label: "Save", age: 10),
    row("o1", "opened TextEdit", verb: "open", age: 20),
  ]

  /// Six of Claude Code's rows in Opie, one of Codex's in Notes, the person's, and one no host named, newest first.
  static var mixedLog: [ActivityLog.Entry] {
    (0..<6)
      .map {
        row(
          "claude-\($0)",
          "clicked Item \($0) in Opie",
          verb: "click",
          app: "Opie",
          label: "Item \($0)",
          age: Double($0),
          session: claude
        )
      } + [
        row("codex", "opened Notes", verb: "open", app: "Notes", age: 10, session: codex),
        ActivityLog.Entry(id: "you", says: "clicked in Opie", actor: .you, ageSeconds: 20, verb: "click", app: "Opie"),
        ActivityLog.Entry(id: "terminal", says: "ran a command", ageSeconds: 30, operation: "run"),
      ]
  }

  /// The moments a pane lists.
  ///
  /// - Parameter activity: The pane.
  /// - Returns: Its moments, in order.
  static func moments(_ activity: ViewerActivityState) -> [ViewerActivityMoment] {
    activity.items.compactMap { item in
      if case .moment(let moment) = item { return moment }
      return nil
    }
  }

  /// The steps a pane lists.
  ///
  /// - Parameter activity: The pane.
  /// - Returns: Its steps, in order.
  static func steps(_ activity: ViewerActivityState) -> [ViewerActivityStep] {
    activity.items.compactMap { item in
      if case .step(let step) = item { return step }
      return nil
    }
  }
}
