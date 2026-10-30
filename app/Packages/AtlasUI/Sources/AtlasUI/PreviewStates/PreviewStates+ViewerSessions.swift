#if DEBUG
  extension PreviewStates {
    /// A session's row as the window draws it, with its tooltip and menu.
    ///
    /// - Parameters:
    ///   - id: The session's key.
    ///   - icon: The host's icon.
    ///   - name: The session's name.
    ///   - line: What it last did.
    ///   - badge: What it does now; none by default.
    ///   - facts: Every fact about it, one a line.
    ///   - recover: Whether it needs the person, so its menu offers Recover…, and whether that can be chosen.
    ///   - cleanup: Whether its menu offers Clean Up This Session's Work…, and whether that can be chosen.
    ///   - receipt: What its last cleanup did, one step a line; none by default.
    ///   - isCleaningUp: Whether its work is being cleaned up now; not by default.
    /// - Returns: The row.
    static func viewerSessionRow(
      _ id: String,
      icon: RowIcon,
      name: String,
      line: String,
      badge: SessionBadge? = nil,
      facts: [String],
      recover: Bool? = nil,
      cleanup: Bool? = nil,
      receipt: [String] = [],
      isCleaningUp: Bool = false
    ) -> ViewerSessionRow {
      ViewerSessionRow(
        id: id,
        icon: icon,
        name: name,
        line: line,
        badge: badge,
        isCleaningUp: isCleaningUp,
        help: viewerSessionHelp(name, line, facts: facts, receipt: receipt),
        menu: viewerSessionMenu(id, facts: facts, recover: recover, cleanup: cleanup)
      )
    }

    /// The live sessions in the service's order: Claude Code with the helper, Codex waiting its turn, Cursor idle since
    /// it opened Opie and Terminal idle since a command took a screenshot 70 seconds ago. The status names each one's
    /// host alone.
    static let sessionRows = [
      viewerSessionRow(
        SessionSamples.claudeKey,
        icon: SessionSamples.claudeIcon,
        name: SessionSamples.claudeName,
        line: SessionSamples.claudeDoing,
        badge: .acting,
        facts: ["Host: Claude Code"]
      ),
      viewerSessionRow(
        SessionSamples.codexKey,
        icon: SessionSamples.codexIcon,
        name: SessionSamples.codexName,
        line: "Waiting for the helper · 8 s",
        badge: .waiting,
        facts: ["Host: Codex"]
      ),
      viewerSessionRow(
        SessionSamples.cursorKey,
        icon: SessionSamples.cursorIcon,
        name: SessionSamples.cursorName,
        line: "Opened Opie · 4 min ago",
        facts: ["Host: Cursor"]
      ),
      viewerSessionRow(
        SessionSamples.terminalKey,
        icon: SessionSamples.terminalIcon,
        name: SessionSamples.terminalName,
        line: "Took a screenshot · 70 s ago",
        facts: ["Host: None named"]
      ),
    ]

    /// Claude Code, whose click may not have finished, under Needs you, and Codex waiting until the person recovers.
    static let needsYouRows = [
      viewerSessionRow(
        SessionSamples.claudeKey,
        icon: SessionSamples.claudeIcon,
        name: SessionSamples.claudeName,
        line: "A click may not have finished",
        badge: .needsYou,
        facts: ["Host: Claude Code"],
        recover: true
      ),
      viewerSessionRow(
        SessionSamples.codexKey,
        icon: SessionSamples.codexIcon,
        name: SessionSamples.codexName,
        line: "Waits until you recover",
        badge: .waiting,
        facts: ["Host: Codex"]
      ),
    ]

    /// The sidebar's sessions while every session's work shows: the three first live sessions' icons over All sessions,
    /// then Claude Code and Codex under Working and Cursor and Terminal under Idle.
    static let allSessions = ViewerSessionsState(
      icons: [SessionSamples.claudeIcon, SessionSamples.codexIcon, SessionSamples.cursorIcon],
      liveCount: sessionRows.count,
      groups: [
        ViewerSessionGroup(.working, rows: Array(sessionRows.prefix(2))),
        ViewerSessionGroup(.idle, rows: Array(sessionRows.suffix(2))),
      ]
    )

    /// Pi · notes-sync, which ended 20 minutes ago.
    static let piEndedRow = viewerSessionRow(
      SessionSamples.piKey,
      icon: SessionSamples.piIcon,
      name: SessionSamples.piName,
      line: "Ended 20 min ago",
      facts: ["Host: Pi", "Folder: notes-sync", "Started: 09:40", "Ended: 20 min ago"]
    )

    /// The sessions by state while the helper needs the person: Claude Code under Needs you, its click unfinished,
    /// Codex under Working, waiting until the person recovers, Cursor and Terminal under Idle, and Ended open on Pi ·
    /// notes-sync.
    static let sessionsByState = ViewerSessionsState(
      icons: [SessionSamples.claudeIcon, SessionSamples.codexIcon, SessionSamples.cursorIcon],
      liveCount: sessionRows.count,
      groups: [
        ViewerSessionGroup(.needsYou, rows: [needsYouRows[0]]),
        ViewerSessionGroup(.working, rows: [needsYouRows[1]]),
        ViewerSessionGroup(.idle, rows: Array(sessionRows.suffix(2))),
        ViewerSessionGroup(.ended, rows: [piEndedRow]),
      ],
      endedCount: 1,
      endedShown: true
    )

    /// Claude Code, Codex, Cursor and a Terminal session live, Claude Code with the helper: the sessions in the
    /// sidebar, the holder named in the header, and its latest action in the footer, led by its icon.
    @MainActor public static let viewerSessions: ViewerState = {
      var state = viewerWorking
      state.sidebar.sessions = allSessions
      state.header = ViewerHeaderState(who: SessionSamples.claudeName, what: "has the helper")
      state.footer.latest = ViewerLatestLine(
        id: "0",
        icon: SessionSamples.claudeIcon,
        words: SessionSamples.claudeDoing,
        age: "just now"
      )
      return state
    }()

    /// The four live sessions and Pi · notes-sync, which ended, folded under Ended with its count.
    @MainActor public static let viewerSessionsEnded: ViewerState = {
      var state = viewerSessions
      state.sidebar.sessions.groups.append(ViewerSessionGroup(.ended, rows: []))
      state.sidebar.sessions.endedCount = 1
      return state
    }()

    /// Claude Code's facts once the sessions' list names it: where and when it started, the apps it opened, the copy
    /// Atlas made, its plan and its screenshots.
    static let claudeRecordFacts = [
      "Host: Claude Code", "Folder: understudy", "Started: 09:12", "Last seen: just now", "Opie: Open · a copy",
      "Notes: Closed", "Copy of Opie", "Environment plan: Applied · can roll back · made at 10:02", "Screenshots: 6",
    ]

    /// Claude Code chosen on the Screen once the sessions' list names it: its row's tooltip and menu list every fact
    /// about it, and its menu offers Clean Up This Session's Work….
    @MainActor public static let viewerSessionChosen: ViewerState = {
      var state = viewerSessions
      state.sidebar.sessions.chosen = .one(SessionSamples.claudeKey)
      state.sidebar.library = viewerLibrary(counts: [.apps: 1])
      state.sidebar.sessions.groups[0].rows[0] = viewerSessionRow(
        SessionSamples.claudeKey,
        icon: SessionSamples.claudeIcon,
        name: SessionSamples.claudeName,
        line: SessionSamples.claudeDoing,
        badge: .acting,
        facts: claudeRecordFacts,
        cleanup: true
      )
      return state
    }()
  }
#endif
