#if DEBUG
  extension PreviewStates {
    /// The helper doesn't answer, so there is no picture: Recover is the way back (MacEmpty).
    @MainActor public static let viewerNotReady = ViewerState(
      top: viewerTop(
        StatusSummary(label: "Not ready", tone: .quiet, mark: .small),
        viewerActions(control: viewerTakeControl(isEnabled: false))
      ),
      screen: ViewerScreenState(
        empty: ViewerEmpty(
          message: EmptyMessageState(
            headline: "Atlas isn't answering in the helper",
            message: "Recover checks the helper and starts it again.",
            action: EmptyMessageAction(title: "Recover…", isPrimary: true)
          ),
          event: .askRecover
        )
      )
    )

    /// The helper is logged out: switching to it to log in is the way back (MacEmpty).
    @MainActor public static let viewerLoggedOut = ViewerState(
      top: viewerTop(
        StatusSummary(label: "Logged out", tone: .quiet, mark: .dot),
        viewerActions(control: viewerTakeControl(isEnabled: false))
      ),
      screen: ViewerScreenState(
        empty: ViewerEmpty(
          message: EmptyMessageState(
            headline: "The helper is logged out",
            message: "Agents can't use it until you log in to it again.",
            action: EmptyMessageAction(title: "Switch to the Helper…", isPrimary: false)
          ),
          event: .askSwitch
        )
      )
    )

    /// The window has just opened: no reading has arrived yet (MacEmpty).
    @MainActor public static let viewerConnecting = ViewerState(
      top: viewerTop(
        StatusSummary(label: "Connecting", tone: .quiet, mark: .dot),
        viewerActions(control: viewerTakeControl(isEnabled: false))
      ),
      screen: ViewerScreenState(
        empty: ViewerEmpty(
          message: EmptyMessageState(
            headline: "Connecting to the helper…",
            message: "This takes a moment after the Mac starts."
          )
        )
      )
    )
  }
#endif
