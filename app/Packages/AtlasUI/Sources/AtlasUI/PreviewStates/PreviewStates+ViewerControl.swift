#if DEBUG
  extension PreviewStates {
    /// The person holds control: the screen's blue edge, and one Give Back in the footer.
    @MainActor public static let viewerControl = ViewerState(
      header: ViewerHeaderState(who: "You", what: "have control of the helper"),
      screen: ViewerScreenState(picture: SampleDesktop.picture, look: .inControl),
      footer: ViewerFooterState(
        latest: viewerControlLine(),
        controls: viewerControls(control: viewerGiveBack, showsPause: false)
      ),
      sidebar: viewerSidebar(helper: viewerHelperReady(logsOut: true))
    )

    /// The person types through an input method in control; the composed text shows before it is sent.
    @MainActor public static let viewerTyping: ViewerState = {
      var state = viewerControl
      state.footer.latest = ViewerLatestLine(id: "typing", words: "Typing こんにちは", caret: true)
      return state
    }()

    /// Take Control still refused once the wait for the agent's step ran out: the reason takes the footer's line.
    @MainActor public static let viewerMidAction: ViewerState = {
      var state = viewerWorking
      state.footer.latest = ViewerLatestLine(
        id: "refusal-1",
        words: "The agent didn't finish its step. Try again.",
        tone: .attention,
        announces: true
      )
      return state
    }()
  }
#endif
