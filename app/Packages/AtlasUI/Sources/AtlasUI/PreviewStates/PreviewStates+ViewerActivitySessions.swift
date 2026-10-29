#if DEBUG
  extension PreviewStates {
    /// Activity with every session's work: each moment named by its session, Claude Code acting in Opie, Codex's
    /// typing refused in Notes, the person's own clicks, Cursor and Terminal before them.
    @MainActor public static let viewerActivityBySession: ViewerState = {
      var state = viewerSessions
      state.sidebar.library = viewerLibrary(.activity, counts: [.activity: 14, .screenshots: 6])
      state.mini = SampleDesktop.picture
        .map { MiniScreenState(picture: $0) }
      state.pane = .activity(bySessionActivity)
      return state
    }()

    /// Activity while Claude Code's click may not have finished: the click held open in its moment with Show Screen
    /// and Recover…, every other session waiting, the recovery alert set aside.
    @MainActor public static let viewerActivityNeedsYou: ViewerState = {
      var state = viewerNeedsYou
      state.alert = nil
      state.sidebar.helper = viewerHelperNeedsYou(alertUp: false)
      state.sidebar.sessions = ViewerSessionsState(
        icons: [SessionSamples.claudeIcon, SessionSamples.codexIcon, SessionSamples.cursorIcon],
        liveCount: 3,
        groups: [
          ViewerSessionGroup(.needsYou, rows: [needsYouRows[0]]),
          ViewerSessionGroup(.working, rows: [needsYouRows[1]]),
          ViewerSessionGroup(.idle, rows: [sessionRows[2]]),
        ]
      )
      state.sidebar.library = viewerLibrary(.activity, counts: [.activity: 14, .screenshots: 6])
      state.footer.latest = ViewerLatestLine(
        id: "c1",
        icon: SessionSamples.claudeIcon,
        words: "Clicked Show Center in Opie",
        age: "just now",
        tone: .attention
      )
      state.mini = SampleDesktop.picture
        .map { MiniScreenState(picture: $0) }
      state.pane = .activity(needsYouActivity)
      return state
    }()

    /// Activity for Claude Code alone, chosen in the sidebar: its moments under its name, Opie open and an older
    /// command folded.
    @MainActor public static let viewerActivityOneSession: ViewerState = {
      var state = viewerActivityBySession
      state.sidebar.sessions.chosen = .one(SessionSamples.claudeKey)
      state.sidebar.library = viewerLibrary(.activity, counts: [.activity: 6, .screenshots: 0])
      state.pane = .activity(oneSessionActivity)
      return state
    }()

    /// One agent's typing refused because another app was in front: a quiet grey callout, no problem, its reason and
    /// the retry that worked.
    @MainActor public static let viewerFocusChanged: ViewerState = {
      var state = viewerIdle
      state.sidebar.library = viewerLibrary(.activity, counts: [.activity: 3])
      state.mini = SampleDesktop.picture
        .map { MiniScreenState(picture: $0) }
      state.pane = .activity(focusActivity)
      return state
    }()
  }
#endif
