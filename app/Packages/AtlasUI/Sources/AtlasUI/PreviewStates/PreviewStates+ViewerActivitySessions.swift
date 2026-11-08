#if DEBUG
  extension PreviewStates {
    /// Activity with every session's work: Claude Code working, Codex's ended session folded, and the person's own
    /// clicks.
    @MainActor public static let viewerActivityBySession: ViewerState = {
      var state = viewerSessions
      state.sidebar.sessions.chosen = .all
      state.mini = SampleDesktop.picture
        .map { MiniScreenState(picture: $0) }
      state.pane = .activity(bySessionActivity)
      return state
    }()

    /// Activity while Claude Code's click may not have finished: the click held open in its run with Show Screen
    /// and Recover…, every other session waiting, the recovery alert set aside.
    @MainActor public static let viewerActivityNeedsYou: ViewerState = {
      var state = viewerNeedsYou
      state.alert = nil
      state.sidebar.helper = viewerHelperNeedsYou(alertUp: false)
      state.sidebar.sessions = ViewerSessionsState(
        chosen: .all,
        icons: [SessionSamples.claudeIcon, SessionSamples.codexIcon, SessionSamples.cursorIcon],
        liveCount: 3,
        groups: [
          ViewerSessionGroup(.needsYou, rows: [needsYouRows[0]]),
          ViewerSessionGroup(.working, rows: [needsYouRows[1]]),
          ViewerSessionGroup(.idle, rows: [sessionRows[2]]),
        ]
      )
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

    /// Activity for Claude Code alone, chosen in the sidebar: its runs at the first level, Chrome opened.
    @MainActor public static let viewerActivityOneSession: ViewerState = {
      var state = viewerActivityBySession
      state.sidebar.sessions.chosen = .one(SessionSamples.claudeKey)
      state.pane = .activity(oneSessionActivity)
      return state
    }()

    /// Activity for Codex alone, chosen in the sidebar after Claude Code: other runs from the first row down.
    @MainActor public static let viewerActivityOtherSession: ViewerState = {
      var state = viewerActivityBySession
      state.sidebar.sessions.chosen = .one(SessionSamples.codexKey)
      state.pane = .activity(otherSessionActivity)
      return state
    }()

    /// One agent's typing refused because another app was in front: a quiet grey callout, no problem, its reason and
    /// the retry that worked.
    @MainActor public static let viewerFocusChanged: ViewerState = {
      var state = viewerIdle
      state.sidebar.sessions.chosen = .all
      state.mini = SampleDesktop.picture
        .map { MiniScreenState(picture: $0) }
      state.pane = .activity(focusActivity)
      return state
    }()
  }
#endif
