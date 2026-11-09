#if DEBUG
  extension PreviewStates {
    /// Activity a moment after the agent moved on to Safari: Safari's run at the top of the live session, its first
    /// step running, and TextEdit's run done.
    @MainActor static let viewerActivityNewRun: ViewerState = {
      var state = viewerActivity
      var pane = workingActivity
      pane.days[0].sessions[0].runs[0].isRunning = false
      pane.days[0].sessions[0].runs[0].time = "01:51–01:58"
      pane.days[0].sessions[0].runs.insert(
        ViewerOutlineRun(
          id: "run.safari",
          icon: sampleApp(SampleAppIcons.safari, "Safari"),
          name: "Safari",
          summary: "Opening Safari · 1 action",
          time: "now",
          isRunning: true
        ),
        at: 0
      )
      state.pane = .activity(pane)
      return state
    }()

  }
#endif
