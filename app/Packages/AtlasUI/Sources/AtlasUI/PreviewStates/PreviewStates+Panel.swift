#if DEBUG
  import SwiftUI

  extension PreviewStates {
    /// Every panel sample, in the order v3's gallery (`#pgallery`) and the renders list them, then a stop the service
    /// refused, which no card draws.
    @MainActor public static var panel: [PreviewState<PanelState>] {
      [
        PreviewState(name: "Idle", value: panelIdle),
        PreviewState(name: "Working", value: panelWorking),
        PreviewState(name: "You have control", value: panelControl),
        PreviewState(name: "Needs you", value: panelNeedsYou),
        PreviewState(name: "Stopped", value: panelStopped),
        PreviewState(name: "Logged out", value: panelLoggedOut),
        PreviewState(name: "Not ready", value: panelNotReady),
        PreviewState(name: "Connecting", value: panelConnecting),
        PreviewState(name: "Paused", value: panelPaused),
        PreviewState(name: "In use elsewhere", value: panelInUseElsewhere),
        PreviewState(name: "Stop refused", value: panelStopRefused),
      ]
    }

    /// The pause button while pictures arrive.
    private static let pictureLive = PanelPause(
      isPaused: false,
      label: "Pause picture",
      help: "Stop updating the helper's screen here"
    )

    /// The pause button once the picture is paused.
    private static let picturePaused = PanelPause(
      isPaused: true,
      label: "Resume picture",
      help: "Update the helper's screen here again"
    )

    /// The helper's screen in a look, with no caption.
    ///
    /// - Parameter look: How the picture looks.
    /// - Returns: The picture's state.
    @MainActor private static func screen(_ look: PanelPictureState.Look) -> PanelPictureState {
      PanelPictureState(look: look, picture: SampleDesktop.picture, pause: pictureLive)
    }

    /// Open Atlas, the main action while nothing needs the person.
    private static let openAtlas = PanelMainButton(title: "Open Atlas", help: "Open Atlas's window", event: .openAtlas)

    /// An agent's actions two minutes ago, newest first, as Recent lists them.
    private static let earlier = [
      recent(0, "Turned Show Center off", "2 min"),
      recent(1, "Clicked Ring in Opie", "2 min"),
      recent(2, "Opened Opie", "2 min"),
    ]

    /// Nothing acts in the helper; its screen shows, with what an agent did last.
    @MainActor static let panelIdle = PanelState(
      status: StatusSummary(label: "Idle", tone: .quiet, mark: .dot, line: "Last: turned Show Center off · 2 min ago"),
      picture: screen(.live),
      recent: earlier,
      controls: PanelControls(main: openAtlas)
    )

    /// An agent acts in the helper: its newest action over the screen, the live row in Recent, and Stop before Open
    /// Atlas.
    @MainActor static let panelWorking = PanelState(
      status: StatusSummary(
        label: "Working",
        tone: .active,
        mark: .pulsing,
        line: "Turning Show Center off in Opie · just now"
      ),
      picture: PanelPictureState(
        look: .live,
        picture: SampleDesktop.picture,
        caption: PanelCaption(text: "Turning Show Center off in Opie", time: "just now"),
        pause: pictureLive
      ),
      recent: [
        recent(0, "Turning Show Center off in Opie", "2 s", isLive: true),
        recent(1, "Clicked Ring in Opie", "9 s"),
        recent(2, "Opened Opie", "31 s"),
      ],
      controls: PanelControls(showsStop: true, main: openAtlas)
    )

    /// The person holds control in Atlas's window: the blue rim, and Give Back.
    @MainActor static let panelControl = PanelState(
      status: StatusSummary(label: "You have control", tone: .link, mark: .dot, line: "In Atlas · the agent waits"),
      picture: screen(.control),
      recent: earlier,
      controls: PanelControls(
        main: PanelMainButton(title: "Give Back", help: "Give control back to the agent", event: .giveBack)
      )
    )

    /// An action may not have finished: the screen greyed under the orange badge, and Recover….
    @MainActor static let panelNeedsYou = PanelState(
      status: StatusSummary(
        label: "Needs you",
        tone: .attention,
        mark: .dot,
        line: "An action may not have finished"
      ),
      picture: screen(.needsYou),
      recent: earlier,
      controls: PanelControls(main: recover)
    )

    /// The person stopped agents: Resume.
    @MainActor static let panelStopped = PanelState(
      status: StatusSummary(label: "Stopped", tone: .attention, mark: .square, line: "The agent waits for you"),
      picture: screen(.live),
      recent: earlier,
      controls: PanelControls(
        main: PanelMainButton(title: "Resume", help: "Let agents act in the helper again", event: .resumeAgent)
      )
    )

    /// The helper is logged out: the mark on the empty stage and no Recent, and Switch to the Helper….
    @MainActor static let panelLoggedOut = PanelState(
      status: StatusSummary(
        label: "Logged out",
        tone: .quiet,
        mark: .dot,
        line: "Agents can't use the helper until you log in"
      ),
      picture: PanelPictureState(look: .empty(message: "The helper is logged out"), pause: pictureLive),
      recent: [],
      controls: PanelControls(
        main: PanelMainButton(
          title: "Switch to the Helper…",
          help: "Lock your screen so you can log in to the helper",
          event: .askSwitch
        )
      )
    )

    /// Atlas isn't answering in the helper: the mark on the empty stage and no Recent, and Recover… waiting.
    @MainActor static let panelNotReady = PanelState(
      status: StatusSummary(
        label: "Not ready",
        tone: .quiet,
        mark: .small,
        line: "Atlas isn't answering in the helper"
      ),
      picture: PanelPictureState(
        look: .empty(message: "Atlas checks again by itself. Settings shows what the helper needs"),
        pause: pictureLive
      ),
      recent: [],
      controls: PanelControls(main: recoverWaiting)
    )

    /// Before the first answer: the mark breathing on the empty stage, and Open Atlas waiting.
    @MainActor static let panelConnecting = PanelState(
      status: StatusSummary(label: "Connecting", tone: .quiet, mark: .dot),
      picture: PanelPictureState(look: .connecting, pause: pictureLive),
      recent: [],
      controls: PanelControls(
        main: PanelMainButton(title: "Open Atlas", isEnabled: false, help: "Open Atlas's window", event: .openAtlas)
      )
    )

    /// The person paused the panel's picture: it greys and darkens with no badge, the pause button shows play, and
    /// the line says work goes on.
    @MainActor static let panelPaused: PanelState = {
      var state = panelIdle
      state.status.line = "Picture paused · work in the helper goes on"
      state.picture = PanelPictureState(look: .paused, picture: SampleDesktop.picture, pause: picturePaused)
      return state
    }()

    /// Another window holds control: Open Atlas.
    @MainActor static let panelInUseElsewhere = PanelState(
      status: StatusSummary(label: "In use elsewhere", tone: .quiet, mark: .dot, line: "Another window has control"),
      picture: screen(.live),
      recent: earlier,
      controls: PanelControls(main: openAtlas)
    )

    /// A stop the service refused: why, in place of the line.
    @MainActor static let panelStopRefused: PanelState = {
      var state = panelWorking
      state.notice = "The agent could not be stopped. Try again."
      return state
    }()

    /// Recover…, the main action while the helper needs recovering.
    private static let recover = PanelMainButton(
      title: "Recover…",
      help: "Check the helper and start its mouse and keyboard again",
      event: .askRecover
    )

    /// Recover…, waiting while the helper isn't ready for a reason Recover can't fix.
    private static let recoverWaiting = PanelMainButton(
      title: "Recover…",
      isEnabled: false,
      help: "Check the helper and start its mouse and keyboard again",
      event: .askRecover
    )

    /// One of Recent's rows: an agent's action that worked.
    ///
    /// - Parameters:
    ///   - id: Its place, newest first.
    ///   - line: What was done.
    ///   - when: How long ago, short.
    ///   - isLive: Whether it is the action running now.
    /// - Returns: The row's entry.
    private static func recent(_ id: Int, _ line: String, _ when: String, isLive: Bool = false) -> PanelRecentEntry {
      PanelRecentEntry(id: String(id), line: line, when: when, isLive: isLive)
    }
  }
#endif
