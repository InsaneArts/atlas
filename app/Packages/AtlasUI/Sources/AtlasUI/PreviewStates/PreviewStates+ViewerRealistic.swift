#if DEBUG
  import CoreGraphics

  extension PreviewStates {
    /// Sessions as weeks of use leave them: a long folder name, one-letter and odd names, two sessions with the same
    /// name, a long line, and twenty ended sessions under Ended, open.
    static var realisticSessions: ViewerSessionsState {
      let claude = SessionSamples.claudeIcon
      let codex = SessionSamples.codexIcon
      let cursor = SessionSamples.cursorIcon
      let live: [(String, RowIcon, String, String, SessionBadge?)] = [
        (
          "claude-long", claude, "atlas-ui-refresh-viewer-polish-second-pass",
          "Clicking “Take Control” in the Atlas viewer’s footer", .acting
        ),
        ("codex-a", codex, "a", "Waiting for the helper · 2 min 41 s", .waiting),
        ("cursor-x", cursor, "x", "Cursor", nil),
        ("terminal-1", SessionSamples.terminalIcon, "Terminal", "", nil),
      ]
      let ended: [(String, RowIcon, String, String)] = [
        ("e1", codex, "demo", "Codex · Thu 11:46"),
        ("e2", claude, "understudy", "Claude Code · Tue 12:44"),
        ("e3", codex, "atlas-t3-r3", "Codex · Wed 22:20"),
        ("e4", codex, "atlas-t3-r3", "Codex · Wed 22:19"),
        ("e5", codex, "atlas-t1-r3", "Codex · Wed 22:18"),
        ("e6", claude, "Ünïcode café — Ω project", "Claude Code · Wed 18:02"),
        ("e7", cursor, "~/Developer/clients/acme/really-deep/folder", "Cursor · Wed 16:40"),
        ("e8", codex, "my project (copy 2)", "Codex · Wed 15:12"),
        ("e9", SessionSamples.piIcon, "notes-sync", "Pi · Wed 09:40"),
        ("e10", claude, "q", "Claude Code · Tue 23:59"),
        ("e11", codex, "atlas-landing", "Codex · Tue 21:03"),
        ("e12", codex, "atlas-landing", "Codex · Tue 20:47"),
        ("e13", claude, "release-0.1.2-notes-and-changelog-draft", "Claude Code · Tue 19:30"),
        ("e14", cursor, "opie", "Cursor · Mon 17:20"),
        ("e15", codex, "billing", "Codex · Mon 15:05"),
        ("e16", claude, "docs", "Claude Code · Mon 11:11"),
        ("e17", SessionSamples.terminalIcon, "Terminal", "Sun 22:41"),
        ("e18", codex, "checkout", "Codex · Sun 10:02"),
        ("e19", claude, "api", "Claude Code · Sat 18:18"),
        ("e20", codex, "dashboard", "Codex · Sat 08:50"),
      ]
      let row = { (sample: (String, RowIcon, String, String, SessionBadge?)) in
        viewerSessionRow(sample.0, icon: sample.1, name: sample.2, line: sample.3, badge: sample.4, facts: [])
      }
      let gone = { (sample: (String, RowIcon, String, String)) in
        viewerSessionRow(sample.0, icon: sample.1, name: sample.2, line: sample.3, facts: [])
      }
      return ViewerSessionsState(
        icons: [claude, codex, cursor],
        liveCount: live.count,
        groups: [
          ViewerSessionGroup(.working, rows: live.prefix(2).map(row)),
          ViewerSessionGroup(.idle, rows: live.suffix(2).map(row)),
          ViewerSessionGroup(.ended, rows: ended.map(gone)),
        ],
        endedCount: ended.count,
        endedShown: true
      )
    }

    /// The Screen with ``realisticSessions`` in the sidebar, the long-named session holding the helper.
    @MainActor public static let viewerSidebarRealistic: ViewerState = {
      var state = viewerWorking
      state.sidebar.sessions = realisticSessions
      state.header = ViewerHeaderState(
        who: "atlas-ui-refresh-viewer-polish-second-pass",
        what: "Claude Code · has the helper"
      )
      return state
    }()

    /// A run whose pictures were lost or came out blank: one still being read, one deleted, one black.
    @MainActor static let realisticPicturesRun = ViewerOutlineRun(
      id: "run.real.pictures",
      icon: sampleApp(SampleAppIcons.textEdit, "TextEdit"),
      name: "TextEdit",
      summary: "Typed into an untitled document with a very long name that goes on · 18 actions",
      time: "13:02–13:10",
      strip: ViewerPictureStrip(
        run: "run.real.pictures",
        tiles: [
          ViewerPictureTile(id: "real-missing-0.png", label: "Screenshot, 13:10"),
          ViewerPictureTile(
            id: "real-blank-1.png",
            thumbnail: SampleDesktop.blank("real-blank-1.png"),
            label: "Screenshot, 13:09"
          ),
          outlineTile("real-2-capture.png", "13:08"),
        ],
        total: 18
      )
    )

    /// A run where nearly everything failed.
    @MainActor static let realisticFailingRun = ViewerOutlineRun(
      id: "run.real.failing",
      icon: sampleApp(SampleAppIcons.safari, "Google Chrome"),
      name: "Google Chrome",
      summary: "Tried to sign in · 7 actions",
      problems: "5 problems",
      time: "12:58–13:01",
      problemSteps: [
        ViewerActivityStep(
          id: "step.real.fail.1",
          kind: .pointer,
          mark: .failed,
          words: ViewerStepWords("Couldn’t press", target: "Continue with a passkey or a security key"),
          time: "13:01",
          hasPicture: true,
          problem: ViewerActivityProblem(why: "The button was covered by a cookie banner that never closed.")
        ),
        ViewerActivityStep(
          id: "step.real.fail.2",
          kind: .typing,
          mark: .failed,
          words: ViewerStepWords("Couldn’t type into", target: "Password"),
          time: "13:00",
          problem: ViewerActivityProblem(why: "No element matched.")
        ),
      ]
    )

    /// Activity for the long-named session: a run with lost and blank pictures and one that kept failing.
    @MainActor public static let viewerActivityRealistic: ViewerState = {
      var state = viewerSidebarRealistic
      state.sidebar.sessions.chosen = .one("claude-long")
      state.mini = SampleDesktop.picture
        .map { MiniScreenState(picture: $0) }
      let session = ViewerOutlineSession(
        id: "session.real",
        icon: SessionSamples.claudeIcon,
        agent: "Claude Code",
        project: "atlas-ui-refresh-viewer-polish-second-pass",
        summary: "TextEdit and Chrome · 25 actions · 18 screenshots",
        problems: "5 problems",
        time: "12:58–now",
        isWorking: true,
        showsHead: false,
        runs: [realisticPicturesRun, realisticFailingRun]
      )
      state.pane = .activity(
        ViewerActivityState(
          session: .one("claude-long"),
          meta: "25 actions today",
          filters: activityFilters(problems: 5, pictures: 18),
          days: [ViewerActivityDay(id: "today", title: "", sessions: [session])]
        )
      )
      return state
    }()
  }
#endif
