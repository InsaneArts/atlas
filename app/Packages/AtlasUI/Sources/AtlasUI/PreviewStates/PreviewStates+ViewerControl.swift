#if DEBUG
  extension PreviewStates {
    /// The person holds control (MacControl).
    @MainActor public static let viewerControl = ViewerState(
      top: viewerTop(
        StatusSummary(label: "You have control", tone: .link, mark: .dot, line: "The agent waits"),
        viewerActions(
          keys: .shown(isOpen: false),
          control: ViewerControlButton(
            title: "Give Back",
            isEnabled: true,
            help: "Give control back (⌃⇧⎋)",
            event: .giveBack
          )
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

    /// Take Control was still refused after its one retry, as an agent's action ran.
    @MainActor public static let viewerMidAction: ViewerState = {
      var state = viewerWorking
      state.top.actions.refusals = 1
      state.toast = "The agent is mid-action. Try again in a moment."
      return state
    }()
  }
#endif
