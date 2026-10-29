#if DEBUG
  extension PreviewStates {
    /// Claude Code chosen before it did anything in the helper: its Activity says what will show up there.
    @MainActor public static let viewerActivityOneSessionEmpty: ViewerState = {
      var state = viewerActivityOneSession
      state.pane = .activity(
        ViewerActivityState(
          title: SessionSamples.claudeName,
          titleIcon: SessionSamples.claudeIcon,
          meta: "",
          query: "",
          filters: oneSessionFilters,
          groups: [],
          empty: EmptyMessageState(
            headline: "Nothing has happened yet",
            message: "What this session does in the helper shows up here."
          ),
          isSearchable: false
        )
      )
      return state
    }()

    /// Claude Code chosen on Screenshots: only the screenshots it took, counted by its name.
    @MainActor public static let viewerShotsOneSession: ViewerState = {
      var state = viewerActivityOneSession
      state.top.tabs = viewerTabs(.screenshots)
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

    /// Claude Code's cleanup being read or run from Details: its button holds a spinner.
    @MainActor public static let viewerSessionCleaningUp: ViewerState = {
      var state = viewerSessionDetail
      state.details?.hostSession?.cleanup?.stage = .working
      return state
    }()

    /// Pi · notes-sync chosen after it ended: Ended in Details' header with how long ago, and when it started and
    /// ended among its facts.
    @MainActor public static let viewerSessionEnded: ViewerState = {
      var state = viewerSessionDetail
      state.top.sessions = ViewerSessionsButton(
        icons: [SessionSamples.piIcon],
        title: SessionSamples.piName,
        help: "Choose whose work to show"
      )
      state.details?.hostSession = ViewerSessionDetail(
        icon: SessionSamples.piIcon,
        name: SessionSamples.piName,
        state: "Ended 20 min ago",
        tone: .quiet,
        facts: [
          Fact(name: "Host", value: "Pi"),
          Fact(name: "Folder", value: "notes-sync"),
          Fact(name: "Started", value: "09:40"),
          Fact(name: "Ended", value: "20 min ago"),
        ]
      )
      return state
    }()
  }
#endif
