#if DEBUG
  import CoreGraphics

  extension PreviewStates {
    /// An agent's three latest actions, oldest first, as captions show them.
    static let workingCaptions = [
      ActivityEntry(
        id: "2",
        line: "Clicked Ring in Opie",
        when: "9 s ago",
        isNewest: false
      ),
      ActivityEntry(id: "1", line: "Scrolled in Opie", when: "5 s ago", isNewest: false),
      ActivityEntry(
        id: "0",
        line: "Turning Show Center off in Opie",
        when: "just now",
        isNewest: true
      ),
    ]

    /// An agent acts in the helper while the person watches.
    @MainActor public static let viewerWorking = ViewerState(
      top: viewerTop(
        // The newest caption on the picture says what the agent is doing, so the line beside the word is quiet.
        StatusSummary(label: "Working", tone: .active, mark: .pulsing, line: ""),
        viewerActions(agent: .stop, control: viewerTakeControl(isEnabled: true))
      ),
      screen: ViewerScreenState(picture: SampleDesktop.picture, captions: workingCaptions)
    )

    /// Nothing acts in the helper; control is there to take.
    @MainActor public static let viewerIdle = ViewerState(
      top: viewerTop(
        StatusSummary(label: "Idle", tone: .quiet, mark: .dot, line: "Last: turned Show Center off · 2 min ago"),
        viewerActions(control: viewerTakeControl(isEnabled: true))
      ),
      screen: ViewerScreenState(picture: SampleDesktop.picture)
    )

    /// The person stopped agents: Resume stands where Stop was.
    @MainActor public static let viewerStopped = ViewerState(
      top: viewerTop(
        StatusSummary(label: "Stopped", tone: .attention, mark: .square, line: "Resume when you're ready"),
        viewerActions(agent: .resume(isEnabled: true), control: viewerTakeControl(isEnabled: true))
      ),
      screen: ViewerScreenState(picture: SampleDesktop.picture)
    )

    /// Another window holds control, so Take Control waits.
    @MainActor public static let viewerInUseElsewhere = ViewerState(
      top: viewerTop(
        StatusSummary(label: "In use elsewhere", tone: .quiet, mark: .dot, line: "Another Atlas viewer has control"),
        viewerActions(control: viewerTakeControl(isEnabled: false))
      ),
      screen: ViewerScreenState(picture: SampleDesktop.picture)
    )

    /// The picture is paused; work in the helper goes on. The status keeps the agent's own line, and the screen's
    /// chip says the picture is paused.
    @MainActor public static let viewerPaused = ViewerState(
      top: viewerTop(
        StatusSummary(label: "Idle", tone: .quiet, mark: .dot, line: "Last: turned Show Center off · 2 min ago"),
        viewerActions(control: viewerTakeControl(isEnabled: true), paused: true)
      ),
      screen: ViewerScreenState(picture: SampleDesktop.picture, look: .paused, chip: .paused)
    )

    /// The paused picture at actual size, one view point a display point: it fades as the fitted one does.
    @MainActor public static let viewerActualSizePaused: ViewerState = {
      var state = viewerPaused
      state.screen.actualSize = CGSize(width: 1440, height: 900)
      return state
    }()
  }
#endif
