#if DEBUG
  extension PreviewStates {
    /// Claude Code's latest actions over the screen, oldest first, each led by its host's icon.
    static let sessionCaptions = [
      ActivityEntry(
        id: "2",
        line: "Clicked Ring in Opie",
        when: "9 s ago",
        isNewest: false,
        icon: SessionSamples.claudeIcon
      ),
      ActivityEntry(
        id: "1",
        line: "Scrolled in Opie",
        when: "5 s ago",
        isNewest: false,
        icon: SessionSamples.claudeIcon
      ),
      ActivityEntry(
        id: "0",
        line: SessionSamples.claudeDoing,
        when: "just now",
        isNewest: true,
        icon: SessionSamples.claudeIcon
      ),
    ]

    /// The sessions button while every session's work shows: the three first live sessions' icons.
    static let allSessionsButton = ViewerSessionsButton(
      icons: [SessionSamples.claudeIcon, SessionSamples.codexIcon, SessionSamples.cursorIcon],
      title: "All sessions",
      help: "Choose whose work to show"
    )

    /// The sessions button while Claude Code's work alone shows.
    static let claudeButton = ViewerSessionsButton(
      icons: [SessionSamples.claudeIcon],
      title: SessionSamples.claudeName,
      help: "Choose whose work to show"
    )

    /// Claude Code, Codex, Cursor and a Terminal session live, Claude Code with the helper: the sessions button beside
    /// the status, the holder named on the screen, and each caption led by Claude Code's icon.
    @MainActor public static let viewerSessions: ViewerState = {
      var state = viewerWorking
      state.top.sessions = allSessionsButton
      state.screen.chip = .holder(name: SessionSamples.claudeName, icon: SessionSamples.claudeIcon)
      state.screen.captions = sessionCaptions
      return state
    }()

    /// The sessions menu open under its button, which draws pressed. The menu itself is AppKit's, drawn by the system.
    @MainActor public static let viewerSessionsMenuOpen: ViewerState = {
      var state = viewerSessions
      state.top.sessions?.isOpen = true
      return state
    }()

    /// Claude Code's lines while it has the helper, its newest four of six.
    @MainActor static let claudeGroupLines = [
      ViewerActivityLine(
        id: "38",
        time: "10:31:42",
        lead: .live,
        kind: .pointer,
        text: SessionSamples.claudeDoing,
        label: "10:31:42, Clicking Show Center in Opie, now"
      ),
      ViewerActivityLine(
        id: "37",
        time: "10:31:38",
        lead: .quiet,
        kind: .scroll,
        text: "Scrolled in Opie",
        label: "10:31:38, Scrolled in Opie"
      ),
      ViewerActivityLine(
        id: "36",
        time: "10:31:33",
        lead: .quiet,
        kind: .pointer,
        text: "Clicked Ring in Opie",
        label: "10:31:33, Clicked Ring in Opie"
      ),
      sessionShotLine,
    ]

    /// Claude Code's screenshot, with its thumbnail.
    @MainActor static let sessionShotLine = ViewerActivityLine(
      id: "35",
      time: "10:31:20",
      lead: .quiet,
      kind: .screenshot,
      text: "Took a screenshot",
      shot: ViewerActivityShot(name: shotNames[2], thumbnail: shotPicture(shotNames[2])),
      label: "10:31:20, Took a screenshot"
    )

    /// Codex's lines: its typing refused while another app was in front, then Notes opened.
    static let codexGroupLines = [
      ViewerActivityLine(
        id: "x2",
        time: "10:30:02",
        lead: .notSent,
        kind: .keys,
        text: "Didn't type in Notes",
        reason: "Another app was in front",
        label: "10:30:02, Didn't type in Notes, Another app was in front"
      ),
      ViewerActivityLine(
        id: "x1",
        time: "10:29:50",
        lead: .quiet,
        kind: .window,
        text: "Opened Notes",
        label: "10:29:50, Opened Notes"
      ),
    ]

    /// The groups after Codex's while every session's work shows: Cursor's, then Terminal's in the status's place
    /// before the person's while the status lists it, else the person's, then Terminal's.
    ///
    /// - Parameter terminalListed: Whether the status lists Terminal.
    /// - Returns: The groups.
    static func laterSessionGroups(terminalListed: Bool) -> [ViewerActivitySessionGroup] {
      let groups = [
        ViewerActivitySessionGroup(
          id: "session-\(SessionSamples.cursorKey)",
          icon: SessionSamples.cursorIcon,
          name: SessionSamples.cursorName,
          state: "Idle",
          lines: [
            ViewerActivityLine(
              id: "u1",
              time: "10:27:42",
              lead: .quiet,
              kind: .window,
              text: "Opened Opie",
              label: "10:27:42, Opened Opie"
            )
          ],
          showAll: .chooseSession(.one(SessionSamples.cursorKey))
        ),
        ViewerActivitySessionGroup(
          id: "you",
          icon: nil,
          name: "You",
          lines: [
            ViewerActivityLine(
              id: "33",
              time: "10:29:40",
              lead: .person,
              kind: .control,
              text: "You gave back control",
              label: "10:29:40, You gave back control"
            ),
            ViewerActivityLine(
              id: "30",
              time: "10:29:05",
              lead: .person,
              kind: .person,
              text: "You clicked in Opie ×3",
              label: "10:29:05, You clicked in Opie ×3"
            ),
          ],
          showAll: .filterActivity(.you)
        ),
        ViewerActivitySessionGroup(
          id: "session-\(SessionSamples.terminalKey)",
          icon: SessionSamples.terminalIcon,
          name: SessionSamples.terminalName,
          state: "Idle",
          lines: [
            ViewerActivityLine(
              id: "t1",
              time: "10:26:42",
              lead: .quiet,
              kind: .command,
              text: "Ran a command",
              label: "10:26:42, Ran a command"
            )
          ],
          showAll: .chooseSession(.one(SessionSamples.terminalKey))
        ),
      ]
      return terminalListed ? [groups[0], groups[2], groups[1]] : groups
    }

    /// Codex's group, waiting its turn or until the person recovers.
    static let codexGroup = ViewerActivitySessionGroup(
      id: "session-\(SessionSamples.codexKey)",
      icon: SessionSamples.codexIcon,
      name: SessionSamples.codexName,
      state: "Waiting",
      lines: codexGroupLines,
      showAll: .chooseSession(.one(SessionSamples.codexKey))
    )

    /// Activity with every session's work, grouped by session in the status's order: Claude Code acting, Codex
    /// waiting with its refused typing, Cursor and Terminal idle, then the person's own lines.
    @MainActor public static let viewerActivityBySession: ViewerState = {
      var state = viewerSessions
      state.top.status.line = "Claude Code · \(SessionSamples.claudeDoing)"
      state.top.tabs = viewerTabs(.activity)
      state.mini = SampleDesktop.picture
        .map { MiniScreenState(picture: $0) }
      let claude = ViewerActivitySessionGroup(
        id: "session-\(SessionSamples.claudeKey)",
        icon: SessionSamples.claudeIcon,
        name: SessionSamples.claudeName,
        state: "Acting",
        tone: .active,
        lines: claudeGroupLines,
        more: "Show all 6",
        moreLabel: "Show all 6 from \(SessionSamples.claudeName)",
        showAll: .chooseSession(.one(SessionSamples.claudeKey))
      )
      state.pane = .activity(
        ViewerActivityState(
          meta: "14 actions today",
          query: "",
          filters: activityFilters(),
          groups: [],
          sessions: [claude, codexGroup] + laterSessionGroups(terminalListed: true)
        )
      )
      return state
    }()

    /// Activity while Claude Code's click may not have finished: its group first, the click flagged with Show Screen
    /// and Recover…, every other session waiting, the recovery alert set aside.
    @MainActor public static let viewerActivityNeedsYou: ViewerState = {
      var state = viewerNeedsYou
      state.alert = nil
      state.top.status.line = "A click by Claude Code may not have finished"
      state.top.sessions = allSessionsButton
      state.top.tabs = viewerTabs(.activity)
      state.screen.captions = [
        sessionCaptions[0],
        sessionCaptions[1],
        ActivityEntry(
          id: "c1",
          line: "Clicked Show Center in Opie",
          when: "just now",
          mark: .failed,
          isNewest: true,
          icon: SessionSamples.claudeIcon
        ),
      ]
      state.mini = SampleDesktop.picture
        .map { MiniScreenState(picture: $0) }
      let unfinished = ViewerActivityLine(
        id: "c1",
        time: "10:31:40",
        lead: .failed,
        kind: .problem,
        text: "Clicked Show Center in Opie",
        reason: "May not have finished. Every session waits until you recover.",
        offersRecovery: true,
        label: "10:31:40, Clicked Show Center in Opie, May not have finished. Every session waits until you recover."
      )
      let claude = ViewerActivitySessionGroup(
        id: "session-\(SessionSamples.claudeKey)",
        icon: SessionSamples.claudeIcon,
        name: SessionSamples.claudeName,
        state: "Needs you",
        tone: .attention,
        lines: [unfinished] + claudeGroupLines.dropFirst(),
        more: "Show all 6",
        moreLabel: "Show all 6 from \(SessionSamples.claudeName)",
        showAll: .chooseSession(.one(SessionSamples.claudeKey))
      )
      state.pane = .activity(
        ViewerActivityState(
          meta: "14 actions today",
          query: "",
          filters: activityFilters(),
          groups: [],
          sessions: [claude, codexGroup] + laterSessionGroups(terminalListed: false)
        )
      )
      return state
    }()

    /// The filters with one session chosen: All and Problems, as Agent and You mean nothing inside one session.
    static let oneSessionFilters = CapsuleTabsState<ViewerActivityFilter>(
      tabs: [CapsuleTab(id: .all, title: "All"), CapsuleTab(id: .problems, title: "Problems")],
      selected: .all,
      label: "Which actions"
    )

    /// Activity for Claude Code alone, chosen in the sessions menu: its lines by time under its name and icon.
    @MainActor public static let viewerActivityOneSession: ViewerState = {
      var state = viewerActivityBySession
      state.top.status.line = SessionSamples.claudeDoing
      state.top.sessions = claudeButton
      let opened = ViewerActivityLine(
        id: "34",
        time: "10:31:12",
        lead: .quiet,
        kind: .window,
        text: "Opened Opie",
        label: "10:31:12, Opened Opie"
      )
      let ran = ViewerActivityLine(
        id: "27",
        time: "10:25:02",
        lead: .quiet,
        kind: .command,
        text: "Ran a command",
        label: "10:25:02, Ran a command"
      )
      state.pane = .activity(
        ViewerActivityState(
          title: SessionSamples.claudeName,
          titleIcon: SessionSamples.claudeIcon,
          meta: "6 actions today",
          query: "",
          filters: oneSessionFilters,
          groups: [
            ViewerActivityGroup(id: "now", title: "Now", lines: claudeGroupLines + [opened]),
            ViewerActivityGroup(id: "today", title: "Earlier today", lines: [ran]),
          ]
        )
      )
      return state
    }()

    /// Claude Code chosen, with Details open on the Screen: its detail leads, then the helper's groups, its login under
    /// Login.
    @MainActor public static let viewerSessionDetail: ViewerState = {
      var state = viewerSessions
      state.top.sessions = claudeButton
      state.top.actions.details = viewerDetailsToggle(shown: true)
      state.details = viewerDetails.details
      state.details?.hostSession = ViewerSessionDetail(
        icon: SessionSamples.claudeIcon,
        name: SessionSamples.claudeName,
        state: "Acting · has the helper",
        tone: .active,
        facts: [
          Fact(name: "Host", value: "Claude Code"),
          Fact(name: "Folder", value: "understudy"),
          Fact(name: "Started", value: "09:12"),
          Fact(name: "Last seen", value: "just now"),
        ],
        apps: [Fact(name: "Opie", value: "Open · a copy"), Fact(name: "Notes", value: "Closed")],
        copies: [Fact(name: "Copy of Opie", value: "")],
        plans: [Fact(name: "Applied · can roll back", value: "Made at 10:02")],
        shots: zip(shotNames, ["10:31:45", "10:31:44", "10:31:20"])
          .map { name, time in
            ViewerActivityShot(name: name, thumbnail: shotPicture(name), label: "Screenshot taken at \(time)")
          },
        allShots: "See all 6",
        cleanup: SessionCleanup(session: SessionSamples.claudeKey, title: "Clean Up This Session's Work…")
      )
      return state
    }()

    /// One agent's typing refused because another app was in front: a quiet grey line, no problem, its reason in
    /// grey, then the typing that followed.
    @MainActor public static let viewerFocusChanged: ViewerState = {
      var state = viewerIdle
      state.top.tabs = viewerTabs(.activity)
      state.mini = SampleDesktop.picture
        .map { MiniScreenState(picture: $0) }
      let lines = [
        ViewerActivityLine(
          id: "f3",
          time: "10:31:40",
          lead: .quiet,
          kind: .keys,
          text: "Typed in Notes",
          label: "10:31:40, Typed in Notes"
        ),
        ViewerActivityLine(
          id: "f2",
          time: "10:31:38",
          lead: .notSent,
          kind: .keys,
          text: "Didn't type in Notes",
          reason: "Another app was in front",
          label: "10:31:38, Didn't type in Notes, Another app was in front"
        ),
        ViewerActivityLine(
          id: "f1",
          time: "10:31:30",
          lead: .quiet,
          kind: .window,
          text: "Opened Notes",
          label: "10:31:30, Opened Notes"
        ),
      ]
      state.pane = .activity(
        ViewerActivityState(
          meta: "3 actions today",
          query: "",
          filters: activityFilters(),
          groups: [ViewerActivityGroup(id: "now", title: "Now", lines: lines)]
        )
      )
      return state
    }()
  }
#endif
