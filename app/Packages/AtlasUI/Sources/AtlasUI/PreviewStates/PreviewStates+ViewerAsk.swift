#if DEBUG
  extension PreviewStates {
    /// An agent works and a question waits for the person, over the Screen they watch: the same answers as the panel's
    /// card in one row.
    @MainActor public static let viewerAsk: ViewerState = {
      var state = viewerWorking
      state.screen.ask = AskCardState(
        id: "v1",
        icon: .mark(.terminal),
        who: "Claude Code · release",
        waited: "0:18",
        title: "Click “Merge pull request” in github.com",
        task: "Task: ship the setup fix",
        choices: AskChoice.allCases,
        scope: "For this task covers Claude Code in github.com until it finishes or for 1 hour.",
        label: "Claude Code · release asks: Click “Merge pull request” in github.com"
      )
      return state
    }()
  }
#endif
