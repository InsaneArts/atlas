#if DEBUG
  extension PreviewStates {
    /// Claude Code chosen before it did anything in the helper: its Activity says what will show up there, and with no
    /// log to read the sessions keep the names the status gives them.
    @MainActor public static let viewerActivityOneSessionEmpty: ViewerState = {
      var state = viewerActivityOneSession
      state.header = viewerSessions.header
      state.sidebar.sessions = allSessions
      state.sidebar.sessions.chosen = .one(SessionSamples.claudeKey)
      state.sidebar.library = viewerLibrary(.activity, counts: [.activity: 0])
      state.pane = .activity(oneSessionEmptyActivity)
      return state
    }()
  }
#endif
