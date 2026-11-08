#if DEBUG
  extension PreviewStates {
    /// The helper doesn't answer, so there is no picture: Settings shows what it needs while Atlas checks again.
    @MainActor public static let viewerNotReady = ViewerState(
      header: ViewerHeaderState(who: "Not ready"),
      screen: ViewerScreenState(
        empty: ViewerEmpty(
          message: EmptyMessageState(
            headline: "Atlas isn't answering in the helper",
            message: "Atlas checks again by itself. Settings shows what the helper needs.",
            action: EmptyMessageAction(title: "Open Settings", isPrimary: true)
          ),
          event: .openSettings
        )
      ),
      footer: ViewerFooterState(
        latest: viewerStateLine("Not ready", "Atlas isn't answering in the helper"),
        controls: viewerControls(control: nil, showsPause: false)
      ),
      sidebar: viewerSidebar(
        helper: ViewerHelperLine(
          tone: .attention,
          title: "Atlas needs setup",
          line: "Helper atlas",
          lineTone: .attention,
          menu: viewerHelperMenu(facts: viewerHelperFacts(newest: "—"))
        )
      )
    )

    /// The helper is logged out: switching to it to log in is the way back.
    @MainActor public static let viewerLoggedOut = ViewerState(
      header: ViewerHeaderState(who: "Logged out"),
      screen: ViewerScreenState(
        empty: ViewerEmpty(
          message: EmptyMessageState(
            headline: "The helper is logged out",
            message: "Agents can't use it until you log in to it again.",
            action: EmptyMessageAction(title: "Switch to the Helper…", isPrimary: true)
          ),
          event: .askSwitch
        )
      ),
      footer: ViewerFooterState(
        latest: viewerStateLine("Logged out", ""),
        controls: viewerControls(control: nil, showsPause: false)
      ),
      sidebar: viewerSidebar(
        helper: ViewerHelperLine(
          tone: .attention,
          title: "The helper is logged out",
          line: "Log in to atlas to go on",
          lineTone: .attention,
          menu: viewerHelperMenu(facts: viewerHelperFacts(newest: "—"))
        )
      )
    )

    /// The picture was lost and the window connects again: the last picture stays behind the words, blurred
    /// and dimmed, and the ring traces round the mark once a moment has passed.
    @MainActor public static let viewerConnecting = ViewerState(
      header: ViewerHeaderState(who: "Connecting"),
      screen: ViewerScreenState(
        empty: ViewerEmpty(
          message: EmptyMessageState(headline: "Reconnecting…", message: ""),
          lastPicture: SampleDesktop.picture,
          showsRing: true
        )
      ),
      footer: ViewerFooterState(
        latest: viewerStateLine("Connecting", "Reconnecting…"),
        controls: viewerControls(control: nil, showsPause: false)
      ),
      sidebar: viewerSidebar(
        helper: ViewerHelperLine(
          tone: .quiet,
          title: "Connecting to Atlas…",
          line: "Helper atlas",
          menu: viewerHelperMenu(facts: viewerHelperFacts(newest: "—"), logsOut: true)
        )
      )
    )
  }
#endif
