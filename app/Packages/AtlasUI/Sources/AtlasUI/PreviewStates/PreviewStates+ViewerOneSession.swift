#if DEBUG
  extension PreviewStates {
    /// Claude Code chosen before it did anything in the helper: its Activity says what will show up there.
    @MainActor public static let viewerActivityOneSessionEmpty: ViewerState = {
      var state = viewerActivityOneSession
      state.sidebar.library = viewerLibrary(.activity, counts: [.activity: 0])
      state.pane = .activity(oneSessionEmptyActivity)
      return state
    }()
  }
#endif
