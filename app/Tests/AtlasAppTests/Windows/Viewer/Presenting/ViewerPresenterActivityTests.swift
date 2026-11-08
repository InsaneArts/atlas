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

  @Test("consecutive actions in one app form one run under its icon, folded under All, its steps never naming the app")
  func actionsInOneAppFormOneRun() throws {
    let snapshot = try ViewerActivityFixtures.onActivity(ViewerActivityFixtures.textEditLog, sessions: [])
    let runs = ViewerActivityFixtures.runs(ViewerPresenter.activity(snapshot))
    #expect(runs.map(\.name) == ["TextEdit"])
    #expect(runs.first?.isOpen == false, "under All a run starts folded")
    #expect(runs.first?.steps.isEmpty == true)
    #expect(runs.first?.icon == .app(.monogram(AppMonogram(name: "TextEdit"))))
    let words = ViewerActivityFixtures.steps(ViewerActivityFixtures.opened(snapshot))
      .map(\.words.text)
    #expect(words == ["Opened TextEdit", "Clicked Save", "Typed into Name", "Pressed ⌘S"], "oldest first")
  }

  @Test("an opened run's steps show their minute only where it changes")
  func stepsShowTheMinuteWhereItChanges() throws {
    let log = [42.0, 72, 112, 117]
      .enumerated()
      .map { index, age in
        ActivityFixtures.row(
          id: "c\(index)",
          says: "clicked Item \(index) in TextEdit",
          ageSeconds: age,
          verb: "click",
          app: "TextEdit",
          label: "Item \(index)"
        )
      }
    let snapshot = try ViewerActivityFixtures.onActivity(log, sessions: [])
    let steps = ViewerActivityFixtures.steps(ViewerActivityFixtures.opened(snapshot))
    #expect(steps.map(\.id) == ["c3", "c2", "c1", "c0"])
    #expect(steps.map(\.time) == ["10:29", "", "10:30", "10:31"])
  }

  @Test("a key press shows its keys as caps")
  func aKeyPressShowsItsKeysAsCaps() throws {
    let steps = ViewerActivityFixtures.steps(
      ViewerActivityFixtures.opened(
        try ViewerActivityFixtures.onActivity(ViewerActivityFixtures.textEditLog, sessions: [])
      )
    )
    let pressed = try #require(steps.last)
    #expect(pressed.words.verb == "Pressed")
    #expect(pressed.words.keys == [["⌘", "S"]])
    #expect(pressed.words.target.isEmpty, "the chord is drawn as caps, never as words too")
    #expect(pressed.kind == .keys)
  }

  @Test("a step that kept a picture of the screen before it offers its picture button, and one without offers none")
  func aStepOffersItsPictureBefore() throws {
    let name = "1792665100000000000-before.jpg"
    let snapshot = try ViewerActivityFixtures.onActivity(
      [
        ActivityFixtures.row(id: "c2", says: "clicked Save in TextEdit", ageSeconds: 1, verb: "click", before: name),
        ActivityFixtures.row(id: "c1", says: "scrolled in TextEdit", ageSeconds: 3, verb: "scroll"),
      ],
      sessions: []
    )
    let steps = ViewerActivityFixtures.steps(ViewerActivityFixtures.opened(snapshot))
    #expect(steps.map(\.id) == ["c1", "c2"])
    #expect(steps.map(\.hasPicture) == [false, true])
    #expect(steps.allSatisfy { $0.inPlace == nil }, "only a screenshot's step shows a picture in place")
  }

  @Test("a typed action names its field only when the row names one")
  func aTypedActionNamesItsFieldOnlyWhenTheRowDoes() throws {
    let steps = ViewerActivityFixtures.steps(
      ViewerActivityFixtures.opened(
        try ViewerActivityFixtures.onActivity(ViewerActivityFixtures.textEditLog, sessions: [])
      )
    )
    #expect(steps[2].words == ViewerStepWords("Typed into", target: "Name"))
    let plain = ViewerActivityFixtures.steps(
      ViewerActivityFixtures.opened(
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
    let activity = ViewerActivityFixtures.opened(try ViewerActivityFixtures.onActivity([row], sessions: []))
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
    let activity = ViewerActivityFixtures.opened(try ViewerActivityFixtures.onActivity(log, sessions: []))
    #expect(
      ViewerActivityFixtures.runs(activity)
        .map(\.name) == ["Commands"]
    )
    #expect(
      ViewerActivityFixtures.steps(activity)
        .map(\.words.text) == ["Ran a command", "A command failed"]
    )
  }

  @Test("a run's row never shows a window's title")
  func aRunsRowNeverShowsAWindowsTitle() throws {
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

  @Test("a folded run says its app, what was done there, its count and its span")
  func aFoldedRunSaysWhatItDid() throws {
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
    let runs = ViewerActivityFixtures.runs(
      ViewerPresenter.activity(try ViewerActivityFixtures.onActivity(log, sessions: []))
    )
    #expect(runs.map(\.name) == ["Notes", "TextEdit"], "newest first")
    let folded = try #require(runs.last)
    #expect(!folded.isOpen)
    #expect(folded.summary == "Clicked · 9 actions")
    #expect(folded.time == "10:21–10:24")
    #expect(folded.label == "TextEdit, Clicked · 9 actions, 10:21–10:24")
  }

  @Test("opening a folded run keeps it open as new actions arrive")
  func openingAFoldedRunKeepsItOpen() throws {
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
    var run = try #require(ViewerActivityFixtures.runs(ViewerPresenter.activity(snapshot)).first)
    #expect(!run.isOpen)
    snapshot.panes.activity.choices[run.id] = true
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
    run = try #require(ViewerActivityFixtures.runs(ViewerPresenter.activity(snapshot)).last)
    #expect(run.name == "TextEdit")
    #expect(run.isOpen, "the person's choice outlasts newer actions")
  }

  @Test("a folded run shows its problem lines and its newest three pictures, with a link naming them all")
  func aFoldedRunShowsProblemsAndPictures() throws {
    let names = (1...5)
      .map { "17926650\(90 + $0)000000000-capture.png" }
    let shots =
      names
      .enumerated()
      .map { index, name in
        ActivityFixtures.row(
          id: "s\(index)",
          says: "took a screenshot",
          ageSeconds: Double(10 - index),
          operation: "screenshot",
          capture: name
        )
      }
    let failed = ActivityFixtures.row(
      id: "r1",
      says: "ran swift build",
      outcome: .failed,
      ageSeconds: 12,
      operation: "run"
    )
    var snapshot = try ViewerActivityFixtures.onActivity(shots.reversed() + [failed], sessions: [])
    snapshot.panes.shots.shots = names.reversed()
      .map { Screenshot(name: $0) }
    let run = try #require(ViewerActivityFixtures.runs(ViewerPresenter.activity(snapshot)).first)
    #expect(!run.isOpen)
    #expect(run.name == "Commands")
    #expect(run.problems == "1 problem")
    #expect(run.problemSteps.map(\.id) == ["r1"])
    #expect(run.problemSteps.first?.mark == .failed)
    let strip = try #require(run.strip)
    #expect(strip.tiles.map(\.id) == Array(names.reversed().prefix(3)), "the newest three")
    #expect(strip.total == 5)
    #expect(strip.hasLink && strip.link == "5 pictures")

    snapshot.panes.activity.pictureRuns[run.id] = false
    let grid = try #require(ViewerActivityFixtures.runs(ViewerPresenter.activity(snapshot)).first?.grid)
    #expect(grid.tiles.count == 5 && grid.more == 0, "a run's pictures opened in place, every one while few")
  }

  @Test("the filters count the runs with problems and with pictures")
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
      ViewerActivityFixtures.runs(ViewerPresenter.activity(snapshot))
        .map(\.name) == ["Notes"]
    )
    snapshot.session.search.query = "nothing like it"
    let none = ViewerPresenter.activity(snapshot)
    #expect(none.days.isEmpty)
    #expect(none.empty?.headline == ViewerWords.noActivityMatch("nothing like it"))
    #expect(none.showsFilters, "a search that finds nothing keeps the filters")
  }

  @Test("one session's pane lists only its runs, under its name, with no row of its own")
  func oneSessionListsOnlyItsRuns() throws {
    var snapshot = try ViewerActivityFixtures.onActivity(ViewerActivityFixtures.mixedLog)
    snapshot.session.chosenSession = .one("3f9a1c07b2e4")
    let activity = ViewerPresenter.activity(snapshot)
    #expect(activity.filters.tabs.map(\.id) == ViewerActivityFilter.allCases)
    #expect(activity.title == "Claude Code · understudy")
    #expect(activity.meta == "6 actions today")
    #expect(
      ViewerActivityFixtures.runs(activity)
        .map(\.name) == ["Opie"]
    )
    #expect(
      ViewerActivityFixtures.sessions(activity).allSatisfy { !$0.showsHead && $0.isOpen },
      "the header names the session already"
    )
  }

  @Test("All sessions preserves historical attribution and VoiceOver names after every session ends")
  func historyKeepsSessionAttribution() throws {
    var snapshot = try ViewerActivityFixtures.onActivity(ViewerActivityFixtures.mixedLog, sessions: [])
    snapshot.panes.activity.log?.sessions = [
      ViewerActivityFixtures.claude: HostSessionSummary(name: "Claude Code · history", host: .claude),
      ViewerActivityFixtures.codex: HostSessionSummary(name: "Codex · history", host: .codex),
    ]
    let sessions = ViewerActivityFixtures.sessions(ViewerPresenter.activity(snapshot))
    let claude = try #require(sessions.first { $0.runs.first?.name == "Opie" || $0.label.contains("Claude Code") })
    let codex = try #require(sessions.first { $0.label.contains("Codex") })
    #expect(claude.icon != nil)
    #expect(codex.icon != nil)
    #expect(claude.label.hasPrefix("Claude Code · history, Opie"))
    #expect(codex.label.hasPrefix("Codex · history, Notes"))
    snapshot.reading.hasStatus = false
    snapshot.session.chosenSession = .one(ViewerActivityFixtures.claude)
    #expect(ViewerActivityFixtures.sessions(ViewerPresenter.activity(snapshot)).allSatisfy { !$0.showsHead })
  }

  @Test("only the causing session's uncertain step offers Recover, and its run and session stay open")
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
    let shown = ViewerPresenter.activity(snapshot)
    snapshot.panes.activity.choices = Dictionary(
      uniqueKeysWithValues: ViewerActivityFixtures.runs(shown).map { ($0.id, false) }
    )
    snapshot.panes.activity.sessionChoices = Dictionary(
      uniqueKeysWithValues: ViewerActivityFixtures.sessions(shown).map { ($0.id, false) }
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
    let held = try #require(ViewerActivityFixtures.runs(activity).first { $0.steps.contains { $0.id == "c1" } })
    #expect(held.isOpen && !held.canFold, "the run holding it offers no fold")
    #expect(
      ViewerActivityFixtures.sessions(activity).first { $0.runs.contains { $0.id == held.id } }?.isOpen == true,
      "the session holding it stays open though folded"
    )
  }

  @Test("a lone session's unresolved action holds its folded run open and offers recovery")
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
    let first = try #require(ViewerActivityFixtures.runs(ViewerPresenter.activity(snapshot)).first)
    snapshot.panes.activity.choices[first.id] = false
    let activity = ViewerPresenter.activity(snapshot)
    #expect(snapshot.reading.statusReading.causer == nil, "one session needs no session label")
    #expect(ViewerActivityFixtures.runs(activity).first?.isOpen == true)
    #expect(ViewerActivityFixtures.runs(activity).first?.canFold == false)
    #expect(
      ViewerActivityFixtures.steps(activity)
        .filter(\.offersRecovery)
        .map(\.id) == ["c1"]
    )
  }

  @Test("an aggregated personal input row has the same action total in its run, Library and VoiceOver summary")
  func personalInputCountsAgreeAcrossTheViewer() throws {
    let row = ActivityFixtures.row(id: "personal", says: "sent 2107 actions", actor: .you, ageSeconds: 0, count: 2107)
    var snapshot = try ViewerActivityFixtures.onActivity([row], sessions: [])
    let activity = ViewerPresenter.activity(snapshot)
    let session = try #require(ViewerActivityFixtures.sessions(activity).first)
    #expect(session.agent == "You")
    #expect(session.summary == ViewerWords.actions(2107), "a run in no app says what was done, its session how much")
    #expect(session.label.contains(ViewerWords.actions(2107)))
    #expect(activity.meta == "2107 actions today")
    #expect(ViewerPresenter.library(snapshot).first { $0.place == .activity }?.count == 2107)
    #expect(ViewerActivityFixtures.runs(activity).first?.isOpen == false)

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
    #expect(ViewerActivityFixtures.sessions(activity).first?.summary.contains("7 actions") == true)
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
    let step = try #require(ViewerActivityFixtures.steps(ViewerActivityFixtures.opened(snapshot)).first)
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
    let step = try #require(ViewerActivityFixtures.steps(ViewerActivityFixtures.opened(snapshot)).first)
    #expect(step.mark == .notSent)
    #expect(step.kind == .typing)
    #expect(step.problem?.why == "Atlas couldn't read which app was in front.")
  }

  @Test("a personal run's follow-up names the person after its step has removed the actor prefix")
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
      ViewerActivityFixtures.opened(try ViewerActivityFixtures.onActivity(log, sessions: []))
    )
    #expect(steps.first?.problem?.next == "Then you clicked Cancel.")
  }

  @Test("sessions group by day, newest first, under a heading only while the outline spans more than one day")
  func sessionsGroupByDay() throws {
    let today = ActivityFixtures.row(id: "t1", says: "opened Notes", ageSeconds: 10, operation: "open", app: "Notes")
    let yesterday = ActivityFixtures.row(
      id: "y1",
      says: "opened Safari",
      ageSeconds: 90_000,
      operation: "open",
      app: "Safari"
    )
    let one = ViewerPresenter.activity(try ViewerActivityFixtures.onActivity([today], sessions: []))
    #expect(one.days.map(\.title) == [""])
    let two = ViewerPresenter.activity(try ViewerActivityFixtures.onActivity([today, yesterday], sessions: []))
    #expect(two.days.map(\.title) == [ViewerWords.todayGroup, ViewerWords.yesterdayGroup])
    #expect(two.days.map { $0.sessions.map(\.summary) } == [["Notes · 1 action"], ["Safari · 1 action"]])
  }

  @Test("a working session opens, an ended one folds to its newest pictures under All, and a filter opens it")
  func sessionsOpenWhileWorkingOrFiltered() throws {
    let shot = "1792665100000000000-capture.png"
    let log = [
      ActivityFixtures.row(
        id: "c1",
        says: "clicked in Opie",
        ageSeconds: 2,
        session: ViewerActivityFixtures.claude,
        verb: "click",
        app: "Opie"
      ),
      ActivityFixtures.row(
        id: "s1",
        says: "took a screenshot",
        ageSeconds: 30,
        operation: "screenshot",
        session: ViewerActivityFixtures.codex,
        capture: shot
      ),
    ]
    var snapshot = try ViewerActivityFixtures.onActivity(log, sessions: [SessionReplies.claudeActing])
    snapshot.panes.shots.shots = [Screenshot(name: shot)]
    let all = ViewerActivityFixtures.sessions(ViewerPresenter.activity(snapshot))
    let claude = try #require(all.first)
    let codex = try #require(all.last)
    #expect(all.count == 2)
    #expect(claude.isWorking && claude.isOpen && !claude.runs.isEmpty)
    #expect(!codex.isWorking && !codex.isOpen && codex.runs.isEmpty)
    #expect(codex.strip?.tiles.map(\.id) == [shot])
    #expect(codex.strip?.opensSession == true)
    snapshot.panes.activity.filter = .pictures
    let pictures = ViewerActivityFixtures.sessions(ViewerPresenter.activity(snapshot))
    #expect(pictures.map(\.id) == [codex.id], "only the session that took a picture")
    #expect(pictures.first?.isOpen == true, "a filter opens the session on what it kept")
    #expect(pictures.first?.runs.first?.isOpen == true, "and its runs")
    snapshot.panes.activity.sessionChoices[codex.id] = false
    #expect(ViewerActivityFixtures.sessions(ViewerPresenter.activity(snapshot)).first?.isOpen == false)
  }

  @Test("a long day still lists the session holding the action of unknown outcome, so its recovery shows")
  func aLongDayKeepsTheHeldSession() throws {
    let newer = (0..<6)
      .map { index in
        ActivityFixtures.row(
          id: "r\(index)",
          says: "clicked in Notes",
          ageSeconds: Double(index * 60),
          session: "5d0c77aa1e0\(index)",
          verb: "click",
          app: "Notes"
        )
      }
    let held = ActivityFixtures.row(
      id: "c1",
      says: "clicked Show Center in Opie",
      outcome: .failed,
      ageSeconds: 900,
      session: ViewerActivityFixtures.claude,
      verb: "click",
      app: "Opie"
    )
    let snapshot = try ViewerActivityFixtures.onActivity(
      newer + [held],
      sessions: [SessionReplies.claudeNeedsYou],
      mode: "recovering",
      state: "needsYou",
      pending: 1
    )
    let day = try #require(ViewerPresenter.activity(snapshot).days.first)
    #expect(day.sessions.count == ViewerWords.mostSessions + 1)
    #expect(day.sessions.last?.runs.first?.steps.first?.offersRecovery == true)
    #expect(day.more?.title == "1 more session today")
  }

  @Test("a long day lists five sessions, then a row naming the rest, which lists them all")
  func aLongDayListsFiveSessions() throws {
    let log = (0..<7)
      .map { index in
        ActivityFixtures.row(
          id: "r\(index)",
          says: "clicked in Notes",
          ageSeconds: Double(index * 60),
          session: "5d0c77aa1e0\(index)",
          verb: "click",
          app: "Notes"
        )
      }
    var snapshot = try ViewerActivityFixtures.onActivity(log, sessions: [])
    let day = try #require(ViewerPresenter.activity(snapshot).days.first)
    #expect(day.sessions.count == ViewerWords.mostSessions)
    let more = try #require(day.more)
    #expect(more.title == "2 more sessions today")
    snapshot.panes.activity.shownDays.insert(more.day)
    let shown = try #require(ViewerPresenter.activity(snapshot).days.first)
    #expect(shown.sessions.count == 7)
    #expect(shown.more == nil)
  }

  @Test("a screenshot opened steps through its run's screenshots, and a step's picture through every step's picture")
  func theSheetStepsThroughItsRun() throws {
    let shot = "1792665100000000000-capture.png"
    let log = [
      ActivityFixtures.row(id: "s1", says: "took a screenshot", ageSeconds: 2, operation: "screenshot", capture: shot),
      ActivityFixtures.row(
        id: "c1",
        says: "ran a command",
        ageSeconds: 5,
        operation: "run",
        before: "1792665097000000000-before.jpg"
      ),
    ]
    var snapshot = try ViewerActivityFixtures.onActivity(log, sessions: [])
    snapshot.panes.shots.shots = [Screenshot(name: shot)]
    let run = try #require(ViewerPresenter.activityMoments(snapshot)?.moments.first)
    snapshot.panes.activity.sheet = ViewerPictureFocus(run: run.id, item: .shot(shot), everyStep: false)
    let fromShot = try #require(ViewerPresenter.activity(snapshot).sheet)
    #expect(fromShot.id == shot)
    #expect(fromShot.title == ViewerStepWords(ViewerWords.tookScreenshot))
    #expect(fromShot.position == "1 of 1", "the run's screenshots alone")
    #expect(!fromShot.hasAfter && fromShot.mark == nil)
    let strip = ViewerActivityFixtures.runs(ViewerPresenter.activity(snapshot)).first?.strip
    #expect(strip?.tiles.first?.isCurrent == true, "its picture in the outline is marked as the one shown")

    snapshot.panes.activity.sheet = ViewerPictureFocus(run: run.id, item: .before("c1"), everyStep: true)
    let fromStep = try #require(ViewerPresenter.activity(snapshot).sheet)
    #expect(fromStep.id == "c1")
    #expect(fromStep.position == "1 of 2", "every step's picture, the screenshot among them")
    #expect(fromStep.canGoForward && !fromStep.canGoBack)
    #expect(fromStep.run == "in this Commands run")
  }

  @Test(
    "stepping the sheet to a picture the folded run does not show gives focus back to the picture it was opened from"
  )
  func theOpenerTakesFocusBackWhenTheShownPictureIsHidden() throws {
    let shots = (0..<5)
      .map { "179266510\($0)000000000-capture.png" }
    let log = shots.enumerated()
      .map { index, shot in
        ActivityFixtures.row(
          id: "s\(index)",
          says: "took a screenshot",
          ageSeconds: Double(10 - index),
          operation: "screenshot",
          capture: shot
        )
      }
    var snapshot = try ViewerActivityFixtures.onActivity(log.reversed(), sessions: [])
    snapshot.panes.shots.shots = shots.map { Screenshot(name: $0) }
    let run = try #require(ViewerPresenter.activityMoments(snapshot)?.moments.first)
    let newest = shots[4]
    snapshot.panes.activity.sheet = ViewerPictureFocus(
      run: run.id,
      item: .shot(newest),
      everyStep: false,
      opener: .shot(newest)
    )
    var tiles = ViewerActivityFixtures.runs(ViewerPresenter.activity(snapshot)).first?.strip?.tiles ?? []
    #expect(tiles.filter(\.isCurrent).map(\.id) == [newest])
    #expect(!tiles.contains { $0.returnsFocus }, "the picture shown takes focus back itself")
    snapshot.panes.activity.sheet?.item = .shot(shots[0])
    tiles = ViewerActivityFixtures.runs(ViewerPresenter.activity(snapshot)).first?.strip?.tiles ?? []
    #expect(!tiles.contains { $0.isCurrent }, "the oldest is not among the three shown")
    #expect(tiles.filter(\.returnsFocus).map(\.id) == [newest])
  }

  @Test("the opener takes focus back by its step's line, or by its screenshot, and nothing else is marked")
  func theOpenerIsMarkedByItsLine() {
    var steps = [
      ViewerActivityStep(id: "c1", kind: .scroll, words: ViewerStepWords("Scrolled"), hasPicture: true),
      ViewerActivityStep(
        id: "s1",
        kind: .screenshot,
        words: ViewerStepWords("Took a screenshot"),
        inPlace: ViewerPictureTile(id: "a.png", label: "")
      ),
    ]
    steps.mark(.before("c1"))
    #expect(steps.map(\.returnsFocus) == [true, false])
    steps.mark(.shot("a.png"))
    #expect(steps.last?.inPlace?.returnsFocus == true)
  }

  @Test("a folded session's picture shown in the sheet keeps the opener from also claiming focus")
  func aFoldedSessionsPictureCounts() throws {
    let shots = (0..<2)
      .map { "179266520\($0)000000000-capture.png" }
    let log =
      shots.enumerated()
      .map { index, shot in
        ActivityFixtures.row(
          id: "s\(index)",
          says: "took a screenshot",
          ageSeconds: Double(10 - index),
          operation: "screenshot",
          session: ViewerActivityFixtures.codex,
          capture: shot
        )
      } + [ActivityFixtures.row(id: "c9", says: "clicked", ageSeconds: 1, session: ViewerActivityFixtures.claude)]
    var snapshot = try ViewerActivityFixtures.onActivity(log.reversed(), sessions: [])
    snapshot.panes.shots.shots = shots.map { Screenshot(name: $0) }
    let run = try #require(ViewerPresenter.activityMoments(snapshot)?.moments.first { $0.shots.count == 2 })
    let codex = try #require(
      ActivitySessionGroup.grouped(ViewerPresenter.activityMoments(snapshot)?.moments ?? []).first {
        $0.stream == .session(ViewerActivityFixtures.codex)
      }
    )
    snapshot.panes.activity.sessionChoices[codex.id] = false
    snapshot.panes.activity.sheet = ViewerPictureFocus(
      run: run.id,
      item: .shot(shots[0]),
      everyStep: false,
      opener: .shot(shots[1])
    )
    let session = try #require(
      ViewerActivityFixtures.sessions(ViewerPresenter.activity(snapshot)).first { !$0.isOpen && $0.strip != nil }
    )
    let tiles = session.strip?.tiles ?? []
    #expect(tiles.filter(\.isCurrent).map(\.id) == [shots[0]])
    #expect(!tiles.contains { $0.returnsFocus }, "only the picture shown takes focus back")
  }
}
