#if DEBUG
  import CoreGraphics
  import Foundation

  extension PreviewStates {
    /// A picture of the outline in a sample: a window capture of the sample desktop, or the whole screen.
    ///
    /// - Parameters:
    ///   - name: Its file name.
    ///   - time: When it was taken.
    ///   - fills: Whether it shows the whole screen.
    ///   - isCurrent: Whether the sheet shows it.
    /// - Returns: The picture.
    @MainActor static func outlineTile(
      _ name: String,
      _ time: String,
      fills: Bool = false,
      isCurrent: Bool = false
    ) -> ViewerPictureTile {
      ViewerPictureTile(
        id: name,
        thumbnail: SampleDesktop.thumbnail(name, display: fills ? helperDisplay : windowCapture),
        isCurrent: isCurrent,
        label: "Screenshot, \(time)"
      )
    }

    /// `count` window pictures of a run, newest first, a few seconds apart.
    ///
    /// - Parameters:
    ///   - run: The run's name, which their file names start with.
    ///   - count: How many.
    /// - Returns: The pictures.
    @MainActor static func outlineTiles(_ run: String, count: Int) -> [ViewerPictureTile] {
      (0..<count)
        .map { outlineTile("\(run)-\($0)-capture.png", "01:5\(9 - $0 % 9)") }
    }

    /// The live TextEdit run: typing and keys for a long while, its newest pictures and the link to all 120.
    @MainActor static let typingRun = ViewerOutlineRun(
      id: "run.textedit.now",
      icon: sampleApp(SampleAppIcons.textEdit, "TextEdit"),
      name: "TextEdit",
      summary: "Typed, pressed keys and clicked · 1,212 actions",
      time: "01:51–now",
      isRunning: true,
      strip: ViewerPictureStrip(run: "run.textedit.now", tiles: outlineTiles("te-now", count: 3), total: 120)
    )

    /// The commands run between the apps, with the two screenshots they took: the whole screen and a window.
    @MainActor static let commandsRun = ViewerOutlineRun(
      id: "run.commands",
      icon: .symbol(.terminal),
      name: "Commands",
      summary: "Ran 3 commands, took 2 screenshots",
      time: "01:50",
      strip: ViewerPictureStrip(
        run: "run.commands",
        tiles: [
          outlineTile("commands-0-capture.png", "01:50", fills: true), outlineTile("commands-1-capture.png", "01:50"),
        ],
        total: 2
      )
    )

    /// The Chrome run's problem: Go could not be pressed, so it was clicked.
    static let chromeProblem = ViewerActivityStep(
      id: "step.chrome.press",
      kind: .pointer,
      mark: .failed,
      words: ViewerStepWords("Couldn’t press", target: "Go"),
      time: "01:50",
      hasPicture: true,
      problem: ViewerActivityProblem(why: "No element matched, so it clicked Go instead.")
    )

    /// The Chrome run's steps, oldest first: a page opened, an address typed and Return pressed, the problem, the
    /// click that did it, and a screenshot of the page.
    @MainActor static let chromeSteps = [
      ViewerActivityStep(
        id: "step.chrome.open",
        kind: .window,
        words: ViewerStepWords("Opened Google Chrome"),
        time: "01:49"
      ),
      ViewerActivityStep(
        id: "step.chrome.type",
        kind: .typing,
        words: ViewerStepWords(
          "Typed",
          quote: "localhost:8080/test.html",
          joiner: "into",
          target: "Address and search bar"
        ),
        hasPicture: true
      ),
      ViewerActivityStep(
        id: "step.chrome.return",
        kind: .keys,
        words: ViewerStepWords("Pressed", keys: [["↩"]], keyName: "Return"),
        hasPicture: true
      ),
      chromeProblem,
      ViewerActivityStep(
        id: "step.chrome.click",
        kind: .pointer,
        words: ViewerStepWords("Clicked", target: "Go"),
        hasPicture: true
      ),
      ViewerActivityStep(
        id: "step.chrome.shot",
        kind: .screenshot,
        words: ViewerStepWords("Took a screenshot"),
        inPlace: outlineTile("chrome-0-capture.png", "01:50")
      ),
    ]

    /// The Chrome run, folded: its problem line and its one picture.
    @MainActor static let chromeRun = ViewerOutlineRun(
      id: "run.chrome",
      icon: sampleApp(SampleAppIcons.safari, "Google Chrome"),
      name: "Google Chrome",
      summary: "Opened a page, typed an address, clicked Go · 6 actions",
      problems: "1 problem",
      time: "01:49–01:50",
      problemSteps: [chromeProblem],
      strip: ViewerPictureStrip(run: "run.chrome", tiles: [outlineTile("chrome-0-capture.png", "01:50")], total: 1)
    )

    /// The Chrome run opened: its steps, oldest first, and Replay.
    @MainActor static var chromeRunOpen: ViewerOutlineRun {
      var run = chromeRun
      run.isOpen = true
      run.canReplay = true
      run.problemSteps = []
      run.steps = chromeSteps
      return run
    }

    /// The earlier TextEdit run, folded with its newest pictures and the link to all 216.
    @MainActor static let olderTypingRun = ViewerOutlineRun(
      id: "run.textedit.before",
      icon: sampleApp(SampleAppIcons.textEdit, "TextEdit"),
      name: "TextEdit",
      summary: "Typed, pressed keys and clicked · 3,826 actions",
      time: "01:08–01:49",
      strip: ViewerPictureStrip(run: "run.textedit.before", tiles: outlineTiles("te-before", count: 3), total: 216)
    )

    /// Claude Code's session today, working: its four runs.
    @MainActor static let claudeSession = ViewerOutlineSession(
      id: "session.claude.today",
      icon: SessionSamples.claudeIcon,
      agent: "Claude Code",
      project: "atlas",
      summary: "TextEdit and Chrome · 5,044 actions · 341 screenshots",
      problems: "1 problem",
      time: "01:08–now",
      isWorking: true,
      runs: [typingRun, commandsRun, chromeRun, olderTypingRun]
    )

    /// Terminal's session yesterday, ended and folded to its one picture of the whole screen.
    @MainActor static let terminalSession = ViewerOutlineSession(
      id: "session.terminal.yesterday",
      icon: SessionSamples.terminalIcon,
      agent: "Terminal",
      summary: "Took a screenshot of the whole screen",
      time: "19:19",
      isOpen: false,
      runs: [],
      strip: ViewerPictureStrip(
        run: "run.terminal",
        tiles: [outlineTile("terminal-0-capture.png", "19:19", fills: true)],
        total: 1
      )
    )

    /// The outline while Claude Code works: today, then yesterday's Terminal.
    @MainActor static let workingActivity = ViewerActivityState(
      meta: "5,044 actions today",
      filters: activityFilters(problems: 1, pictures: 339),
      days: [
        ViewerActivityDay(id: "today", title: "Today", sessions: [claudeSession]),
        ViewerActivityDay(id: "yesterday", title: "Yesterday", sessions: [terminalSession]),
      ]
    )

    /// The outline with the live TextEdit run's pictures opened in place.
    @MainActor static var shotsActivity: ViewerActivityState {
      var session = claudeSession
      session.runs[0].strip = nil
      session.runs[0].grid = ViewerPictureGrid(
        run: typingRun.id,
        tiles: outlineTiles("te-now", count: 8),
        more: 112
      )
      var activity = workingActivity
      activity.days[0].sessions = [session]
      return activity
    }

    /// Claude Code alone: its runs at the first level, Chrome opened.
    @MainActor static var oneSessionActivity: ViewerActivityState {
      var session = claudeSession
      session.showsHead = false
      session.runs[2] = chromeRunOpen
      return ViewerActivityState(
        title: "Typing in TextEdit",
        session: .one(SessionSamples.claudeKey),
        meta: "5,044 actions today",
        filters: activityFilters(problems: 1, pictures: 339),
        days: [ViewerActivityDay(id: "today", title: "", sessions: [session])]
      )
    }

    /// Codex alone, chosen in the sidebar: its runs in Safari, Notes and between them at the first level.
    @MainActor static var otherSessionActivity: ViewerActivityState {
      let runs = [
        ViewerOutlineRun(
          id: "run.codex.safari",
          icon: sampleApp(SampleAppIcons.safari, "Safari"),
          name: "Safari",
          summary: "Opened the landing page and scrolled · 14 actions",
          time: "10:12–10:14",
          strip: ViewerPictureStrip(run: "run.codex.safari", tiles: outlineTiles("codex-safari", count: 3), total: 9)
        ),
        ViewerOutlineRun(
          id: "run.codex.commands",
          icon: .symbol(.terminal),
          name: "Commands",
          summary: "Ran 2 commands",
          time: "10:11"
        ),
        ViewerOutlineRun(
          id: "run.codex.notes",
          icon: sampleApp(SampleAppIcons.notes, "Notes"),
          name: "Notes",
          summary: "Typed the release notes · 6 actions",
          time: "10:08–10:10",
          strip: ViewerPictureStrip(run: "run.codex.notes", tiles: outlineTiles("codex-notes", count: 2), total: 2)
        ),
      ]
      let session = ViewerOutlineSession(
        id: "session.codex.landing",
        icon: SessionSamples.codexIcon,
        agent: "Codex",
        project: "atlas-landing",
        summary: "Safari and Notes · 22 actions",
        time: "10:08–10:14",
        showsHead: false,
        runs: runs
      )
      return ViewerActivityState(
        title: "atlas-landing",
        session: .one(SessionSamples.codexKey),
        meta: "22 actions today",
        filters: activityFilters(pictures: 11),
        days: [ViewerActivityDay(id: "today", title: "", sessions: [session])]
      )
    }

    /// Every session's work with Chrome opened.
    @MainActor static var openedActivity: ViewerActivityState {
      var session = claudeSession
      session.runs[2] = chromeRunOpen
      var activity = workingActivity
      activity.days[0].sessions = [session]
      return activity
    }

    /// Problems: only the Chrome run, opened to its problem.
    @MainActor static var problemsActivity: ViewerActivityState {
      var run = chromeRunOpen
      run.steps = [chromeProblem]
      var session = claudeSession
      session.runs = [run]
      return ViewerActivityState(
        meta: "5,044 actions today",
        filters: activityFilters(.problems, problems: 1, pictures: 339),
        days: [ViewerActivityDay(id: "today", title: "", sessions: [session])]
      )
    }

    /// An ended benchmark session in the samples, folded to its pictures.
    ///
    /// - Parameters:
    ///   - index: Its place, which names it.
    ///   - time: When it ran.
    /// - Returns: The session.
    @MainActor static func benchmarkSession(_ index: Int, _ time: String) -> ViewerOutlineSession {
      ViewerOutlineSession(
        id: "session.codex.\(index)",
        icon: SessionSamples.codexIcon,
        agent: "Codex",
        project: "atlas-t\(index % 4 + 1)-r\(index % 3 + 1)",
        summary: "TextEdit · \(index % 3 + 3) actions · \(index % 2 == 0 ? "1 screenshot" : "2 screenshots")",
        time: time,
        isOpen: false,
        runs: [],
        strip: ViewerPictureStrip(
          run: "run.codex.\(index)",
          tiles: outlineTiles("codex-\(index)", count: index % 2 == 0 ? 1 : 2),
          total: index % 2 == 0 ? 1 : 2
        )
      )
    }

    /// Days before today: yesterday's Terminal, then Wednesday's benchmark sessions and the row for the rest.
    @MainActor static let daysActivity = ViewerActivityState(
      meta: "",
      filters: activityFilters(pictures: 52),
      days: [
        ViewerActivityDay(id: "yesterday", title: "Yesterday", sessions: [terminalSession]),
        ViewerActivityDay(
          id: "wednesday",
          title: "Wednesday",
          sessions: [benchmarkSession(1, "22:20"), benchmarkSession(2, "22:18"), benchmarkSession(3, "22:17")],
          more: ViewerMoreSessions(day: "wednesday", title: "46 more sessions on Wednesday")
        ),
      ]
    )

    /// A long list: today's live session, then a day of many ended sessions.
    @MainActor static let manyActivity = ViewerActivityState(
      meta: "5,044 actions today",
      filters: activityFilters(problems: 1, pictures: 339),
      days: [
        ViewerActivityDay(id: "today", title: "Today", sessions: [claudeSession]),
        ViewerActivityDay(
          id: "wednesday",
          title: "Wednesday",
          sessions: (1...5).map { benchmarkSession($0, "22:\(25 - $0)") },
          more: ViewerMoreSessions(day: "wednesday", title: "43 more sessions on Wednesday")
        ),
      ]
    )

    /// Every session's work: Claude Code working, Codex's ended session folded, and the person's own clicks.
    @MainActor static let bySessionActivity = ViewerActivityState(
      meta: "38 actions today",
      filters: activityFilters(problems: 1, pictures: 6),
      days: [
        ViewerActivityDay(
          id: "today",
          title: "",
          sessions: [
            claudeSession,
            benchmarkSession(1, "10:12"),
            ViewerOutlineSession(
              id: "session.you.today",
              icon: nil,
              agent: "You",
              summary: "Opie · 2 actions",
              time: "09:58",
              isOpen: false,
              runs: []
            ),
          ]
        )
      ]
    )

    /// The click that may not have finished, held open in its run with Show Screen and Recover….
    @MainActor static var needsYouActivity: ViewerActivityState {
      let uncertain = ViewerActivityStep(
        id: "step.opie.click",
        kind: .pointer,
        mark: .failed,
        words: ViewerStepWords("Clicked", target: "Show Center"),
        time: "10:31",
        hasPicture: true,
        problem: ViewerActivityProblem(why: "It may not have finished. Every session waits until you recover."),
        offersRecovery: true
      )
      let run = ViewerOutlineRun(
        id: "run.opie",
        icon: sampleApp(SampleAppIcons.opie, "Opie"),
        name: "Opie",
        summary: "Clicked · 1 action",
        problems: "1 problem",
        time: "10:31",
        isOpen: true,
        canFold: false,
        steps: [uncertain]
      )
      var session = claudeSession
      session.runs = [run]
      return ViewerActivityState(
        meta: "14 actions today",
        filters: activityFilters(problems: 1, pictures: 6),
        days: [ViewerActivityDay(id: "today", title: "", sessions: [session])]
      )
    }

    /// One agent's typing refused because another app was in front: no problem, its reason in grey.
    @MainActor static var focusActivity: ViewerActivityState {
      let refused = ViewerActivityStep(
        id: "step.notes.refused",
        kind: .typing,
        mark: .notSent,
        words: ViewerStepWords("Didn’t type in Notes"),
        time: "10:31",
        problem: ViewerActivityProblem(why: "Another app was in front.", next: "The agent tried again, and it worked.")
      )
      let run = ViewerOutlineRun(
        id: "run.notes",
        icon: sampleApp(SampleAppIcons.notes, "Notes"),
        name: "Notes",
        summary: "Typed · 2 actions",
        time: "10:31",
        isOpen: true,
        steps: [
          refused,
          ViewerActivityStep(
            id: "step.notes.typed",
            kind: .typing,
            words: ViewerStepWords("Typed", quote: "Ring settings", joiner: "into", target: "Note"),
            hasPicture: true
          ),
        ]
      )
      var session = claudeSession
      session.showsHead = false
      session.runs = [run]
      return ViewerActivityState(
        meta: "3 actions today",
        filters: activityFilters(),
        days: [ViewerActivityDay(id: "today", title: "", sessions: [session])]
      )
    }

    /// Claude Code chosen before it did anything in the helper.
    @MainActor static let oneSessionEmptyActivity = ViewerActivityState(
      title: SessionSamples.claudeTitle,
      session: .one(SessionSamples.claudeKey),
      meta: "",
      filters: activityFilters(),
      days: [],
      empty: EmptyMessageState(
        headline: "Nothing has happened yet",
        message: "What this session does in the helper shows up here."
      ),
      showsFilters: false
    )

    /// The sheet over a screenshot of the live TextEdit run: the newest of its 120.
    @MainActor static let pictureSheet = ViewerPictureSheet(
      id: "te-now-0-capture.png",
      icon: sampleApp(SampleAppIcons.textEdit, "TextEdit"),
      title: ViewerStepWords("Took a screenshot"),
      line: "TextEdit · 01:58:41 · Claude Code · atlas",
      picture: SampleDesktop.thumbnail("te-now-0-capture.png", display: windowCapture),
      aspect: windowCapture,
      position: "120 of 120",
      run: "in this TextEdit run",
      canGoBack: true,
      canGoForward: false,
      canShowInFinder: true
    )

    /// The sheet over the screen just before Return was pressed in Chrome, with Before and After.
    @MainActor static let stepPictureSheet = ViewerPictureSheet(
      id: "step.chrome.return",
      icon: sampleApp(SampleAppIcons.safari, "Google Chrome"),
      title: ViewerStepWords("Pressed", keys: [["↩"]], keyName: "Return"),
      line: "Google Chrome · 01:50:02, just before this step · Claude Code · atlas",
      picture: SampleDesktop.signIn("step.chrome.return"),
      aspect: CGSize(width: 1440, height: 900),
      mark: ViewerActionMark(shape: .keys([["↩"]], name: "Return"), spot: CGPoint(x: 0.5, y: 0.4)),
      hasAfter: true,
      position: "2 of 5",
      run: "in this Google Chrome run",
      canGoBack: true,
      canGoForward: true,
      canShowInFinder: true
    )
  }
#endif
