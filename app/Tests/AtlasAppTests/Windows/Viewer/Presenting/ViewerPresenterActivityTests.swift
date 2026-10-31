import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("ViewerPresenter+Activity")
struct ViewerPresenterActivityTests {
  @Test("a failed Activity read replaces the empty state and any earlier rows until a read succeeds")
  func aReadFailureTakesPrecedence() throws {
    for rows in [[], ViewerActivityFixtures.textEditLog] {
      var snapshot = try ViewerActivityFixtures.onActivity(rows, sessions: [])
      snapshot.panes.activity.failure = "The activity log could not be read."
      let failed = ViewerPresenter.activity(snapshot)
      #expect(failed.empty?.headline == ViewerWords.activityFailed)
      #expect(failed.empty?.message == "The activity log could not be read.")
      #expect(failed.days.isEmpty)
      #expect(!failed.showsFilters)
      snapshot.panes.activity.failure = nil
      let recovered = ViewerPresenter.activity(snapshot)
      #expect(recovered.empty?.headline != ViewerWords.activityFailed)
      #expect(recovered.days.isEmpty == rows.isEmpty)
    }
  }

  @Test("each kind of action has its glyph, and a problem has the warning glyph")
  func eachKindHasItsGlyph() {
    let rows: [(entry: ActivityLog.Entry, kind: ViewerActivityKind)] = [
      (ActivityFixtures.row(id: "1", says: "clicked Save in TextEdit", verb: "click"), .pointer),
      (ActivityFixtures.row(id: "2", says: "typed in TextEdit", verb: "type"), .typing),
      (ActivityFixtures.row(id: "3", says: "pressed ⌘S in TextEdit", verb: "key"), .keys),
      (ActivityFixtures.row(id: "4", says: "scrolled in Opie"), .scroll),
      (ActivityFixtures.row(id: "5", says: "took a screenshot", operation: "screenshot"), .screenshot),
      (ActivityFixtures.row(id: "6", says: "opened Opie", operation: "open"), .window),
      (ActivityFixtures.row(id: "7", says: "ran a command", operation: "run"), .command),
      (ActivityFixtures.row(id: "8", says: "clicked in Opie", actor: .you), .person),
      (ActivityFixtures.row(id: "9", says: "gave back control", actor: .you, operation: "release"), .control),
      (ActivityFixtures.row(id: "10", says: "couldn't click Save in TextEdit", outcome: .failed), .problem),
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
    let activity = ViewerPresenter.activity(
      try ViewerActivityFixtures.onActivity(ViewerActivityFixtures.textEditLog, sessions: [])
    )
    let moments = ViewerActivityFixtures.moments(activity)
    #expect(moments.map(\.title) == ["TextEdit"])
    #expect(moments.first?.isOpen == true)
    #expect(moments.first?.icon == .app(.monogram(AppMonogram(name: "TextEdit"))))
    let words = ViewerActivityFixtures.steps(activity)
      .map(\.words.text)
    #expect(words == ["Pressed ⌘S", "Typed into Name", "Clicked Save", "Opened TextEdit"])
  }

  @Test("a key press shows its keys as caps")
  func aKeyPressShowsItsKeysAsCaps() throws {
    let steps = ViewerActivityFixtures.steps(
      ViewerPresenter.activity(try ViewerActivityFixtures.onActivity(ViewerActivityFixtures.textEditLog, sessions: []))
    )
    let pressed = try #require(steps.first)
    #expect(pressed.words.verb == "Pressed")
    #expect(pressed.words.keys == [["⌘", "S"]])
    #expect(pressed.words.target.isEmpty, "the chord is drawn as caps, never as words too")
    #expect(pressed.kind == .keys)
  }

  @Test("a typed action names its field only when the row names one")
  func aTypedActionNamesItsFieldOnlyWhenTheRowDoes() throws {
    let steps = ViewerActivityFixtures.steps(
      ViewerPresenter.activity(try ViewerActivityFixtures.onActivity(ViewerActivityFixtures.textEditLog, sessions: []))
    )
    #expect(steps[1].words == ViewerStepWords("Typed into", target: "Name"))
    let plain = ViewerActivityFixtures.steps(
      ViewerPresenter.activity(
        try ViewerActivityFixtures.onActivity(
          [ActivityFixtures.row(id: "t1", says: "typed in TextEdit", ageSeconds: 0, verb: "type", app: "TextEdit")],
          sessions: []
        )
      )
    )
    #expect(plain.first?.words == ViewerStepWords("Typed"))
  }

  @Test(
    "app operations recorded as run preserve their safe operation words",
    arguments: ["open", "open-link", "close", "install", "cleanup"]
  )
  func appOperationsKeepTheirWords(_ verb: String) throws {
    let words = try #require(
      [
        "open": "opened TextEdit", "open-link": "opened a link", "close": "closed TextEdit",
        "install": "installed TextEdit", "cleanup": "cleaned up this session's work",
      ][verb]
    )
    let row = ActivityFixtures.row(
      id: "app",
      says: words,
      operation: "run",
      verb: verb,
      app: "TextEdit"
    )
    let activity = ViewerPresenter.activity(try ViewerActivityFixtures.onActivity([row], sessions: []))
    #expect(ViewerActivityFixtures.steps(activity).first?.words.text == Words.leadingCapital(words))
    #expect(ViewerActivityFixtures.steps(activity).first?.kind == .window)
  }

  @Test("a command shows no arguments")
  func aCommandShowsNoArguments() throws {
    let log = [
      ActivityFixtures.row(
        id: "r2",
        says: "ran swift build --verbose",
        outcome: .failed,
        ageSeconds: 1,
        operation: "run"
      ),
      ActivityFixtures.row(id: "r1", says: "ran rm -rf build", ageSeconds: 5, operation: "run"),
    ]
    let activity = ViewerPresenter.activity(try ViewerActivityFixtures.onActivity(log, sessions: []))
    #expect(
      ViewerActivityFixtures.moments(activity)
        .map(\.title) == ["Commands"]
    )
    #expect(
      ViewerActivityFixtures.steps(activity)
        .map(\.words.text) == ["A command failed", "Ran a command"]
    )
  }

  @Test("a moment's header never shows a window's title")
  func aMomentsHeaderNeverShowsAWindowsTitle() throws {
    var snapshot = try ViewerActivityFixtures.onActivity(ViewerActivityFixtures.textEditLog, sessions: [])
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
      .map {
        ActivityFixtures.row(
          id: "e\($0)",
          says: "clicked in TextEdit",
          ageSeconds: Double(600 - $0 * 20),
          verb: "click",
          app: "TextEdit"
        )
      }
    let log =
      [
        ActivityFixtures.row(
          id: "n1",
          says: "opened Notes",
          ageSeconds: 2,
          operation: "open",
          verb: "open",
          app: "Notes"
        )
      ] + older
    let moments = ViewerActivityFixtures.moments(
      ViewerPresenter.activity(try ViewerActivityFixtures.onActivity(log, sessions: []))
    )
    let folded = try #require(moments.last)
    #expect(folded.title == "TextEdit")
    #expect(!folded.isOpen)
    #expect(folded.span == "9 actions · 10:21–10:24")
    #expect(folded.age == "7 min")
  }

  @Test("opening a folded moment keeps it open as new actions arrive")
  func openingAFoldedMomentKeepsItOpen() throws {
    let older = (0..<3)
      .map {
        ActivityFixtures.row(
          id: "e\($0)",
          says: "clicked in TextEdit",
          ageSeconds: Double(600 + $0 * 10),
          verb: "click",
          app: "TextEdit"
        )
      }
    var snapshot = try ViewerActivityFixtures.onActivity(older, sessions: [])
    var moment = try #require(ViewerActivityFixtures.moments(ViewerPresenter.activity(snapshot)).first)
    snapshot.panes.activity.choices[moment.id] = false
    #expect(ViewerActivityFixtures.moments(ViewerPresenter.activity(snapshot)).first?.isOpen == false)
    snapshot.panes.activity.choices[moment.id] = true
    let newer = ActivityFixtures.row(
      id: "e9",
      says: "clicked in TextEdit",
      ageSeconds: 590,
      verb: "click",
      app: "TextEdit"
    )
    snapshot.panes.activity.log = ActivityLog(
      entries: [
        ActivityFixtures.row(
          id: "n1",
          says: "opened Notes",
          ageSeconds: 0,
          operation: "open",
          verb: "open",
          app: "Notes"
        ), newer,
      ] + older
    )
    moment = try #require(ViewerActivityFixtures.moments(ViewerPresenter.activity(snapshot)).last)
    #expect(moment.title == "TextEdit")
    #expect(moment.isOpen, "the person's choice outlasts newer actions")
  }

  @Test("the filters count the problems and screenshots there are")
  func theFiltersCountWhatWasDone() throws {
    let log =
      ViewerActivityFixtures.textEditLog + [
        ActivityFixtures.row(id: "s1", says: "took a screenshot", ageSeconds: 40, operation: "screenshot"),
        ActivityFixtures.row(
          id: "f1",
          says: "couldn't click Save in TextEdit",
          outcome: .failed,
          ageSeconds: 50,
          verb: "click",
          app: "TextEdit"
        ),
      ]
    let activity = ViewerPresenter.activity(try ViewerActivityFixtures.onActivity(log, sessions: []))
    #expect(activity.filters.tabs.map(\.count) == [nil, 1, 1, nil])
  }

  @Test("the Activity search follows the sidebar's query while Activity shows")
  func theActivitySearchFollowsTheSidebar() throws {
    var snapshot = try ViewerActivityFixtures.onActivity(
      ViewerActivityFixtures.mixedLog,
      sessions: [SessionReplies.claudeActing]
    )
    snapshot.session.search.query = "notes"
    #expect(
      ViewerActivityFixtures.moments(ViewerPresenter.activity(snapshot))
        .map(\.title) == ["Notes"]
    )
    snapshot.session.search.query = "nothing like it"
    let none = ViewerPresenter.activity(snapshot)
    #expect(none.days.isEmpty)
    #expect(none.empty?.headline == ViewerWords.noActivityMatch("nothing like it"))
    #expect(none.showsFilters, "a search that finds nothing keeps the filters")
  }

  @Test("one session's pane lists only its moments, under its name")
  func oneSessionListsOnlyItsMoments() throws {
    var snapshot = try ViewerActivityFixtures.onActivity(ViewerActivityFixtures.mixedLog)
    snapshot.session.chosenSession = .one("3f9a1c07b2e4")
    let activity = ViewerPresenter.activity(snapshot)
    #expect(activity.filters.tabs.map(\.id) == ViewerActivityFilter.allCases)
    #expect(activity.title == "Claude Code · understudy")
    #expect(activity.meta == "6 actions today")
    #expect(
      ViewerActivityFixtures.moments(activity)
        .map(\.title) == ["Opie"]
    )
    #expect(
      ViewerActivityFixtures.moments(activity).allSatisfy { $0.session == nil },
      "one session's moments need no name"
    )
  }

  @Test("All sessions preserves historical attribution and VoiceOver names after every session ends")
  func historyKeepsSessionAttribution() throws {
    var snapshot = try ViewerActivityFixtures.onActivity(ViewerActivityFixtures.mixedLog, sessions: [])
    snapshot.panes.activity.log?.sessions = [
      ViewerActivityFixtures.claude: HostSessionSummary(name: "Claude Code · history", host: .claude),
      ViewerActivityFixtures.codex: HostSessionSummary(name: "Codex · history", host: .codex),
    ]
    let moments = ViewerActivityFixtures.moments(ViewerPresenter.activity(snapshot))
    let claude = try #require(moments.first { $0.title == "Opie" && $0.session?.name == "Claude Code · history" })
    let codex = try #require(moments.first { $0.title == "Notes" })
    #expect(claude.session?.icon != nil)
    #expect(codex.session?.icon != nil)
    #expect(claude.label.contains("Claude Code · history"))
    #expect(codex.label.contains("Codex · history"))
    snapshot.reading.hasStatus = false
    snapshot.session.chosenSession = .one(ViewerActivityFixtures.claude)
    #expect(ViewerActivityFixtures.moments(ViewerPresenter.activity(snapshot)).allSatisfy { $0.session == nil })
  }

  @Test("only the causing session's uncertain step offers Recover, and its moment stays open")
  func onlyTheCausersUncertainStepOffersRecover() throws {
    let log = [
      ActivityFixtures.row(
        id: "c1",
        says: "clicked Show Center in Opie",
        outcome: .failed,
        ageSeconds: 0,
        session: ViewerActivityFixtures.claude,
        verb: "click",
        app: "Opie"
      ),
      ActivityFixtures.row(
        id: "x1",
        says: "couldn't click Save in Notes",
        outcome: .failed,
        ageSeconds: 1,
        session: ViewerActivityFixtures.codex,
        verb: "click",
        app: "Notes"
      ),
    ]
    var snapshot = try ViewerActivityFixtures.onActivity(
      log,
      sessions: [SessionReplies.claudeNeedsYou, SessionReplies.codexWaiting(recovery: true)],
      mode: "recovering",
      state: "needsYou",
      pending: 1
    )
    snapshot.panes.activity.choices = Dictionary(
      uniqueKeysWithValues: ViewerActivityFixtures.moments(ViewerPresenter.activity(snapshot)).map { ($0.id, false) }
    )
    let activity = ViewerPresenter.activity(snapshot)
    let offering = ViewerActivityFixtures.steps(activity)
      .filter(\.offersRecovery)
      .map(\.id)
    #expect(offering == ["c1"])
    #expect(
      ViewerActivityFixtures.steps(activity).first?.problem?.why
        == "May not have finished. Every session waits until you recover."
    )
    #expect(ViewerActivityFixtures.moments(activity).first?.canFold == false, "the moment holding it offers no fold")
  }

  @Test("a lone session's unresolved action holds its folded moment open and offers recovery")
  func oneSessionCannotFoldAwayRecovery() throws {
    var snapshot = try ViewerActivityFixtures.onActivity(
      [
        ActivityFixtures.row(
          id: "c1",
          says: "clicked in Opie",
          outcome: .failed,
          ageSeconds: 0,
          session: ViewerActivityFixtures.claude,
          verb: "click",
          app: "Opie"
        )
      ],
      sessions: [SessionReplies.claudeNeedsYou],
      mode: "recovering",
      state: "needsYou",
      pending: 1
    )
    let first = try #require(ViewerActivityFixtures.moments(ViewerPresenter.activity(snapshot)).first)
    snapshot.panes.activity.choices[first.id] = false
    let activity = ViewerPresenter.activity(snapshot)
    #expect(snapshot.reading.statusReading.causer == nil, "one session needs no session label")
    #expect(ViewerActivityFixtures.moments(activity).first?.isOpen == true)
    #expect(ViewerActivityFixtures.moments(activity).first?.canFold == false)
    #expect(
      ViewerActivityFixtures.steps(activity)
        .filter(\.offersRecovery)
        .map(\.id) == ["c1"]
    )
  }

  @Test("an aggregated personal input row has the same action total in its moment, Library and VoiceOver summary")
  func personalInputCountsAgreeAcrossTheViewer() throws {
    let row = ActivityFixtures.row(id: "personal", says: "sent 2107 actions", actor: .you, ageSeconds: 0, count: 2107)
    var snapshot = try ViewerActivityFixtures.onActivity([row], sessions: [])
    let activity = ViewerPresenter.activity(snapshot)
    #expect(ViewerActivityFixtures.moments(activity).first?.span.hasPrefix("2107 actions ·") == true)
    #expect(activity.meta == "2107 actions today")
    #expect(ViewerPresenter.library(snapshot).first { $0.place == .activity }?.count == 2107)

    let moment = try #require(ViewerActivityFixtures.moments(activity).first)
    snapshot.panes.activity.choices[moment.id] = false
    let folded = try #require(ViewerActivityFixtures.moments(ViewerPresenter.activity(snapshot)).first)
    #expect(!folded.isOpen)
    #expect(folded.label.contains("2107 actions"))

    snapshot.panes.activity.filter = .problems
    snapshot.session.search.query = "no match"
    let filtered = ViewerPresenter.activity(snapshot)
    #expect(filtered.days.isEmpty)
    #expect(filtered.meta == activity.meta)
    #expect(ViewerPresenter.library(snapshot).first { $0.place == .activity }?.count == 2107)
  }

  @Test("a session's Library and VoiceOver summary count its actions and exclude personal input")
  func sessionActionCountsAgreeAcrossTheViewer() throws {
    let rows = [
      ActivityFixtures.row(id: "chosen", ageSeconds: 0, count: 7, session: ViewerActivityFixtures.claude),
      ActivityFixtures.row(id: "other", ageSeconds: 1, count: 3, session: ViewerActivityFixtures.codex),
      ActivityFixtures.row(id: "personal", actor: .you, ageSeconds: 2, count: 2107),
    ]
    var snapshot = try ViewerActivityFixtures.onActivity(rows)
    snapshot.session.chosenSession = .one(ViewerActivityFixtures.claude)
    let activity = ViewerPresenter.activity(snapshot)
    #expect(activity.meta == "7 actions today")
    #expect(ViewerActivityFixtures.moments(activity).first?.span.hasPrefix("7 actions ·") == true)
    #expect(ViewerPresenter.library(snapshot).first { $0.place == .activity }?.count == 7)
  }

  @Test("a refusal before anything was sent is no problem, and says what the agent did next")
  func aRefusalIsNoProblem() throws {
    let log = [
      ActivityFixtures.row(id: "f2", says: "typed in Notes", ageSeconds: 1, verb: "type", app: "Notes"),
      ActivityFixtures.row(
        id: "f1",
        says: "didn't type in Notes",
        outcome: .notSent,
        ageSeconds: 3,
        reason: "Another app was in front",
        verb: "type",
        app: "Notes"
      ),
    ]
    var snapshot = try ViewerActivityFixtures.onActivity(log, sessions: [])
    let step = try #require(ViewerActivityFixtures.steps(ViewerPresenter.activity(snapshot)).last)
    #expect(step.mark == .notSent)
    #expect(step.kind == .typing)
    #expect(
      step.problem
        == ViewerActivityProblem(why: "Another app was in front.", next: "Then it typed.")
    )
    snapshot.panes.activity.filter = .problems
    #expect(ViewerPresenter.activity(snapshot).days.isEmpty)
  }

  @Test("an unreadable focus refusal says Atlas could not read the app in front")
  func anUnreadableFocusRefusalSaysAtlasCouldNotReadTheAppInFront() throws {
    let log = [
      ActivityFixtures.row(
        id: "unreadable",
        says: "didn't type in Notes",
        outcome: .notSent,
        ageSeconds: 1,
        reason: "Atlas couldn't read which app was in front",
        verb: "type",
        app: "Notes"
      )
    ]
    let snapshot = try ViewerActivityFixtures.onActivity(log, sessions: [])
    let step = try #require(ViewerActivityFixtures.steps(ViewerPresenter.activity(snapshot)).first)
    #expect(step.mark == .notSent)
    #expect(step.kind == .typing)
    #expect(step.problem?.why == "Atlas couldn't read which app was in front.")
  }

  @Test("a personal moment's follow-up names the person after its step has removed the actor prefix")
  func personalFollowUpsKeepTheirActor() throws {
    let log = [
      ActivityFixtures.row(
        id: "next",
        says: "clicked Cancel in Notes",
        actor: .you,
        ageSeconds: 1,
        verb: "click",
        app: "Notes",
        label: "Cancel"
      ),
      ActivityFixtures.row(
        id: "failed",
        says: "couldn't click Save in Notes",
        outcome: .failed,
        actor: .you,
        ageSeconds: 3,
        verb: "click",
        app: "Notes",
        label: "Save"
      ),
    ]
    let steps = ViewerActivityFixtures.steps(
      ViewerPresenter.activity(try ViewerActivityFixtures.onActivity(log, sessions: []))
    )
    #expect(steps.last?.problem?.next == "Then you clicked Cancel.")
  }

}
