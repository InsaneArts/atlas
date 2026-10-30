#if DEBUG
  import CoreGraphics

  extension PreviewStates {
    /// An agent acts in the helper while the person watches: the footer says what it is doing now.
    @MainActor public static let viewerWorking = ViewerState(
      header: ViewerHeaderState(who: "Working", tone: .active),
      screen: ViewerScreenState(picture: SampleDesktop.picture, menu: viewerScreenMenu()),
      footer: ViewerFooterState(
        latest: ViewerLatestLine(id: "0", words: "Turning Show Center off in Opie", age: "just now"),
        controls: viewerControls(agent: .stop, control: viewerTakeControl(isEnabled: true))
      ),
      sidebar: viewerSidebar(helper: viewerHelperReady(logsOut: false))
    )

    /// Nothing acts in the helper; control is there to take.
    @MainActor public static let viewerIdle = ViewerState(
      header: ViewerHeaderState(who: "Idle"),
      screen: ViewerScreenState(picture: SampleDesktop.picture, menu: viewerScreenMenu()),
      footer: ViewerFooterState(
        latest: viewerStateLine("Idle", "Last: turned Show Center off · 2 min ago"),
        controls: viewerControls(control: viewerTakeControl(isEnabled: true))
      ),
      sidebar: viewerSidebar(helper: viewerHelperReady(logsOut: true))
    )

    /// The person stopped agents: Resume stands where Stop was.
    @MainActor public static let viewerStopped = ViewerState(
      header: ViewerHeaderState(who: "Stopped", tone: .attention),
      screen: ViewerScreenState(picture: SampleDesktop.picture, menu: viewerScreenMenu()),
      footer: ViewerFooterState(
        latest: viewerStateLine("Stopped", "Resume when you're ready"),
        controls: viewerControls(agent: .resume(isEnabled: true), control: viewerTakeControl(isEnabled: true))
      ),
      sidebar: viewerSidebar(helper: viewerHelperReady(logsOut: true))
    )

    /// Another window holds control, so Take Control waits.
    @MainActor public static let viewerInUseElsewhere = ViewerState(
      header: ViewerHeaderState(who: "In use elsewhere"),
      screen: ViewerScreenState(picture: SampleDesktop.picture, menu: viewerScreenMenu()),
      footer: ViewerFooterState(
        latest: viewerStateLine("In use elsewhere", "Another Atlas viewer has control"),
        controls: viewerControls(control: viewerTakeControl(isEnabled: false))
      ),
      sidebar: viewerSidebar(helper: viewerHelperReady(logsOut: true))
    )

    /// The picture is paused; work in the helper goes on. The header says so, and the footer keeps the helper's own
    /// line.
    @MainActor public static let viewerPaused = ViewerState(
      header: ViewerHeaderState(who: "Paused", what: "picture from 10:31"),
      screen: ViewerScreenState(picture: SampleDesktop.picture, look: .paused, menu: viewerScreenMenu(paused: true)),
      footer: ViewerFooterState(
        latest: viewerStateLine("Idle", "Last: turned Show Center off · 2 min ago"),
        controls: viewerControls(control: viewerTakeControl(isEnabled: true), paused: true)
      ),
      sidebar: viewerSidebar(helper: viewerHelperReady(logsOut: true, picture: "Paused"))
    )

    /// The paused picture at actual size, one view point a display point: it fades as the fitted one does.
    @MainActor public static let viewerActualSizePaused: ViewerState = {
      var state = viewerPaused
      state.screen.actualSize = CGSize(width: 1440, height: 900)
      state.screen.menu = viewerScreenMenu(paused: true, actualSize: true)
      return state
    }()
  }
#endif
