#if DEBUG
  extension PreviewStates {
    /// Agents stopped, but two background jobs could not be: the notice says they may still run.
    @MainActor static let panelStoppedJobsLeft: PanelState = {
      var state = panelStopped
      state.notice = "Agents stopped, but 2 background jobs couldn't be stopped. They may still run in the helper."
      return state
    }()

    /// The person holds control while a command an agent started still runs, which control does not hold back.
    @MainActor static let panelControlCommandRuns: PanelState = {
      var state = panelControl
      state.status.line = "A command an agent started is still running"
      return state
    }()

    /// An action may not have finished while a command an agent started still runs: recovery waits for it, so the
    /// main button opens Atlas instead of Recover….
    @MainActor static let panelNeedsYouCommandRuns: PanelState = {
      var state = panelNeedsYou
      state.status.line = "An action may not have finished; recovery waits for a running command"
      state.controls.main = PanelMainButton(title: "Open Atlas", help: "Open Atlas's window", event: .openAtlas)
      return state
    }()
  }
#endif
