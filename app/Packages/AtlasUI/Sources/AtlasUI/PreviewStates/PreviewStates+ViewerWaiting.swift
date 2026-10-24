#if DEBUG
  extension PreviewStates {
    /// The helper doesn't answer, so there is no picture: Recover is the way back.
    @MainActor public static let viewerNotReady = ViewerState(
      top: viewerTop(
        StatusSummary(label: "Not ready", tone: .quiet, mark: .small, line: "Atlas isn't answering in the helper"),
        viewerActions(control: nil, showsPause: false, showsMore: false)
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

    /// The helper is logged out: switching to it to log in is the way back.
    @MainActor public static let viewerLoggedOut = ViewerState(
      top: viewerTop(
        StatusSummary(label: "Logged out", tone: .quiet, mark: .dot),
        viewerActions(control: nil, showsPause: false, showsMore: false)
      ),
      screen: ViewerScreenState(
        empty: ViewerEmpty(
          message: EmptyMessageState(
            headline: "The helper is logged out",
            message: "Agents can't use it until you log in to it again.",
            action: EmptyMessageAction(title: "Switch to the Helper…", isPrimary: true)
          ),
          event: .askSwitch
        )
      )
    )

    /// The picture was lost and the window connects again: the last picture stays behind the words, blurred
    /// and dimmed, and the ring traces round the mark once a moment has passed.
    @MainActor public static let viewerConnecting = ViewerState(
      top: viewerTop(
        StatusSummary(label: "Connecting", tone: .quiet, mark: .dot, line: "Reconnecting…"),
        viewerActions(control: nil, showsPause: false, showsMore: false)
      ),
      screen: ViewerScreenState(
        empty: ViewerEmpty(
          message: EmptyMessageState(headline: "Reconnecting…", message: ""),
          lastPicture: SampleDesktop.picture,
          showsRing: true
        )
      )
    )
  }
#endif
