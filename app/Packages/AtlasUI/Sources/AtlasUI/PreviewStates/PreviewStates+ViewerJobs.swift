#if DEBUG
  extension PreviewStates {
    /// Agents stopped, but two background jobs could not be: the notice says they may still run.
    @MainActor public static let viewerStoppedJobsLeft: ViewerState = {
      var state = viewerStopped
      state.screen.notice =
        "Agents stopped, but 2 background jobs couldn't be stopped. They may still run in the helper."
      return state
    }()

    /// The person holds control while a command an agent started still runs, which control does not hold back.
    @MainActor public static let viewerControlCommandRuns: ViewerState = {
      var state = viewerControl
      state.top.status.line = "A command an agent started is still running"
      return state
    }()

    /// The person holds control while a background job may still run: Stop stays, as control ends no job.
    @MainActor public static let viewerControlJobRuns: ViewerState = {
      var state = viewerControl
      state.top.actions.agent = .stop
      state.top.actions.agentHelp = "Stop Agent (⌘.)"
      return state
    }()
  }
#endif
