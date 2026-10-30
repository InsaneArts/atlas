#if DEBUG
  extension PreviewStates {
    /// Agents stopped, but two background jobs could not be: the footer's notice says they may still run.
    @MainActor public static let viewerStoppedJobsLeft: ViewerState = {
      var state = viewerStopped
      let notice = "Agents stopped, but 2 background jobs couldn't be stopped. They may still run in the helper."
      state.footer.latest = ViewerLatestLine(id: "notice-\(notice)", words: notice, announces: true)
      return state
    }()

    /// The person holds control while a command an agent started still runs, which control does not hold back.
    @MainActor public static let viewerControlCommandRuns: ViewerState = {
      var state = viewerControl
      state.footer.latest = viewerControlLine("A command an agent started is still running")
      return state
    }()

    /// The person holds control while a background job may still run: Stop stays, as control ends no job.
    @MainActor public static let viewerControlJobRuns: ViewerState = {
      var state = viewerControl
      state.footer.controls.agent = .stop
      state.footer.controls.agentHelp = "Stop Agent (⌘.)"
      return state
    }()
  }
#endif
