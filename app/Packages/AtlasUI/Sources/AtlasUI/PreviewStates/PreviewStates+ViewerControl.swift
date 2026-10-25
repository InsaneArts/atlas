#if DEBUG
  extension PreviewStates {
    /// The person holds control.
    @MainActor public static let viewerControl = ViewerState(
      top: viewerTop(
        StatusSummary(label: "You have control", tone: .link, mark: .dot, line: "The agent waits"),
        viewerActions(
          keys: .shown(isOpen: false),
          control: ViewerControlButton(
            title: "Give Back",
            isEnabled: true,
            help: "Give Back Control (⌃⇧⎋)",
            event: .giveBack,
            titles: viewerControlTitles
          ),
          showsPause: false
        ),
        isInControl: true
      ),
      screen: ViewerScreenState(picture: SampleDesktop.picture, look: .inControl, chip: .inControl)
    )

    /// The person types through an input method in control; the composed text shows before it is sent.
    @MainActor public static let viewerTyping: ViewerState = {
      var state = viewerControl
      state.screen.chip = .typing("こんにちは")
      return state
    }()

    /// Take Control still refused once the wait for the agent's step ran out.
    @MainActor public static let viewerMidAction: ViewerState = {
      var state = viewerWorking
      state.top.actions.refusals = 1
      state.toast = "The agent didn't finish its step. Try again."
      return state
    }()
  }
#endif
