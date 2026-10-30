#if DEBUG
  extension PreviewStates {
    /// Claude Code chosen before it did anything in the helper: its Activity says what will show up there.
    @MainActor public static let viewerActivityOneSessionEmpty: ViewerState = {
      var state = viewerActivityOneSession
      state.sidebar.library = viewerLibrary(.activity, counts: [.activity: 0, .screenshots: 0])
      state.pane = .activity(oneSessionEmptyActivity)
      return state
    }()

    /// Claude Code chosen on Screenshots: only the screenshots it took, counted by its name. The sessions' list names
    /// it, so its row lists the list's facts, and its menu offers Clean Up This Session's Work….
    @MainActor public static let viewerShotsOneSession: ViewerState = {
      var state = viewerActivityOneSession
      state.sidebar.sessions.groups[0].rows[0] = viewerSessionRow(
        SessionSamples.claudeKey,
        icon: SessionSamples.claudeIcon,
        name: SessionSamples.claudeName,
        line: SessionSamples.claudeDoing,
        badge: .acting,
        facts: [
          "Host: Claude Code", "Folder: understudy", "Started: 09:12", "Last seen: 41 min ago", "Opie", "Notes",
          "Copy of Opie", "Environment plan: Applied · can roll back · made at 10:02", "Screenshots: 6",
        ],
        cleanup: true
      )
      state.sidebar.library = viewerLibrary(.screenshots, counts: [.activity: 6, .screenshots: 6])
      state.mini = viewerShots.mini
      if case .screenshots(var shots) = viewerShots.pane {
        shots.meta = "6 by \(SessionSamples.claudeName)"
        state.pane = .screenshots(shots)
      }
      return state
    }()

    /// Claude Code chosen on Screenshots before it took any: none by its name, and what will show up there.
    @MainActor public static let viewerShotsOneSessionEmpty: ViewerState = {
      var state = viewerShotsOneSession
      state.sidebar.sessions = viewerActivityOneSession.sidebar.sessions
      state.sidebar.library = viewerLibrary(.screenshots, counts: [.activity: 6, .screenshots: 0])
      state.pane = .screenshots(
        ViewerShotsState(
          meta: "None by \(SessionSamples.claudeName)",
          days: [],
          empty: EmptyMessageState(
            headline: "No screenshots yet",
            message: "Screenshots this session takes show up here."
          )
        )
      )
      return state
    }()
  }
#endif
