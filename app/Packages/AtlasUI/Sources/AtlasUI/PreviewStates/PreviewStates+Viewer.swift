#if DEBUG
  import CoreGraphics

  extension PreviewStates {
    /// The standard size for main window samples, so renders compare at the same dimensions.
    public static let viewerSize = CGSize(width: 1440, height: 900)

    /// Every main window sample, in the order the previews list them: each of the helper's states, each alert and each
    /// empty message, then each list's pane, then the host sessions' states.
    @MainActor public static var viewer: [PreviewState<ViewerState>] {
      [
        PreviewState(name: "Working", value: viewerWorking),
        PreviewState(name: "Waiting for you", value: viewerAsk),
        PreviewState(name: "Signing in", value: viewerSigningIn),
        PreviewState(name: "Signed in", value: viewerSignedIn),
        PreviewState(name: "Forgot a site", value: viewerForgotten),
        PreviewState(name: "Idle", value: viewerIdle),
        PreviewState(name: "You have control", value: viewerControl),
        PreviewState(name: "Needs you", value: viewerNeedsYou),
        PreviewState(name: "Stopped", value: viewerStopped),
        PreviewState(name: "In use elsewhere", value: viewerInUseElsewhere),
        PreviewState(name: "Paused", value: viewerPaused),
        PreviewState(name: "Helper needs setup", value: viewerHelperNeedsSetup),
        PreviewState(name: "Not ready", value: viewerNotReady),
        PreviewState(name: "Logged out", value: viewerLoggedOut),
        PreviewState(name: "Connecting", value: viewerConnecting),
        PreviewState(name: "Switch", value: viewerSwitch),
        PreviewState(name: "Log out", value: viewerLogOut),
        PreviewState(name: "Log out anyway", value: viewerLogOutAnyway),
        PreviewState(name: "Restart", value: viewerRestart),
        PreviewState(name: "Typing", value: viewerTyping),
        PreviewState(name: "Mid-action", value: viewerMidAction),
        PreviewState(name: "Actual size, paused", value: viewerActualSizePaused),
        PreviewState(name: "Actual size, needs you", value: viewerActualSizeNeedsYou),
        PreviewState(name: "Activity", value: viewerActivity),
        PreviewState(name: "Activity, nothing yet", value: viewerActivityEmpty),
        PreviewState(name: "Activity, paused", value: viewerActivityPaused),
        PreviewState(name: "Activity, a run opened", value: viewerActivityOpened),
        PreviewState(name: "Activity, problems", value: viewerActivityProblems),
        PreviewState(name: "Activity, days before", value: viewerActivityDays),
        PreviewState(name: "Activity, many sessions", value: viewerActivityMany),
        PreviewState(name: "Activity, a run's pictures", value: viewerActivityShots),
        PreviewState(name: "A picture", value: viewerPictureSheet),
        PreviewState(name: "A step's picture", value: viewerStepPicture),
        PreviewState(name: "Sessions", value: viewerSessions),
        PreviewState(name: "Sessions, one ended", value: viewerSessionsEnded),
        PreviewState(name: "Activity by session", value: viewerActivityBySession),
        PreviewState(name: "Activity, needs you", value: viewerActivityNeedsYou),
        PreviewState(name: "Activity, one session", value: viewerActivityOneSession),
        PreviewState(name: "Session chosen", value: viewerSessionChosen),
        PreviewState(name: "Clean up", value: viewerCleanUp),
        PreviewState(name: "Session cleaned up", value: viewerSessionCleanedUp),
        PreviewState(name: "Focus changed", value: viewerFocusChanged),
        PreviewState(name: "Stopped, jobs left", value: viewerStoppedJobsLeft),
        PreviewState(name: "You have control, a command runs", value: viewerControlCommandRuns),
        PreviewState(name: "You have control, a job may run", value: viewerControlJobRuns),
        PreviewState(name: "Activity, one session, nothing yet", value: viewerActivityOneSessionEmpty),
        PreviewState(name: "Session cleaning up", value: viewerSessionCleaningUp),
        PreviewState(name: "Session ended", value: viewerSessionEnded),
        PreviewState(name: "Sidebar by state", value: viewerSidebarByState),
        PreviewState(name: "Sidebar, Ended folded", value: viewerSidebarEndedFolded),
        PreviewState(name: "Sidebar, search open", value: viewerSidebarSearch),
        PreviewState(name: "Sidebar hidden", value: viewerSidebarHidden),
      ]
    }

    /// Take Control, as offered, with its hand.
    ///
    /// - Parameter isEnabled: Whether it can be taken now.
    /// - Returns: The button.
    static func viewerTakeControl(isEnabled: Bool) -> ViewerControlButton {
      ViewerControlButton(
        title: "Take Control",
        symbol: .handRaised,
        isEnabled: isEnabled,
        help: "Take Control (⌃⇧⎋)",
        event: .takeControl,
        labels: viewerMainLabels
      )
    }

    /// Give Back, in the accent, while this window holds control.
    static let viewerGiveBack = ViewerControlButton(
      title: "Give Back",
      look: .accent,
      isEnabled: true,
      help: "Give Back Control (⌃⇧⎋)",
      event: .giveBack,
      labels: viewerMainLabels
    )

    /// Recover…, while the helper needs the person.
    static let viewerRecover = ViewerControlButton(
      title: "Recover…",
      isEnabled: true,
      help: "Check the helper and start its mouse and keyboard again",
      event: .askRecover,
      labels: viewerMainLabels
    )

    /// Every label the main capsule takes in turn, as the presenter lists them.
    static let viewerMainLabels = [
      ViewerControlLabel("Take Control", symbol: .handRaised),
      ViewerControlLabel("Connecting…", symbol: .handRaised),
      ViewerControlLabel("Give Back"),
      ViewerControlLabel("Recover…"),
    ]

    /// The pause button.
    ///
    /// - Parameter paused: Whether the picture is paused.
    /// - Returns: The toggle.
    static func viewerPause(paused: Bool = false) -> ViewerToggle {
      paused
        ? ViewerToggle(isOn: true, label: "Resume Picture", help: "Resume Picture (⇧⌘P)")
        : ViewerToggle(isOn: false, label: "Pause Picture", help: "Pause Picture (⇧⌘P)")
    }

    /// The footer's buttons.
    ///
    /// - Parameters:
    ///   - agent: Stop or Resume; neither by default.
    ///   - control: The main capsule; nil hides it.
    ///   - paused: Whether the picture is paused.
    ///   - showsPause: Whether there is a picture to pause.
    /// - Returns: The buttons.
    static func viewerControls(
      agent: ViewerAgentButton? = nil,
      control: ViewerControlButton?,
      paused: Bool = false,
      showsPause: Bool = true
    ) -> ViewerFooterControls {
      ViewerFooterControls(
        pause: showsPause ? viewerPause(paused: paused) : nil,
        agent: agent,
        agentHelp: agentHelp(agent),
        control: control
      )
    }

    /// Stop's or Resume's tooltip.
    ///
    /// - Parameter agent: Stop or Resume.
    /// - Returns: Its tooltip, with its shortcut; empty for neither.
    private static func agentHelp(_ agent: ViewerAgentButton?) -> String {
      switch agent {
      case .stop: "Stop Agent (⌘.)"
      case .resume: "Resume Agent (⌘.)"
      case nil: ""
      }
    }

    /// The footer's line while it says the helper's state.
    ///
    /// - Parameters:
    ///   - label: The state's word.
    ///   - words: Its line.
    /// - Returns: The line.
    static func viewerStateLine(_ label: String, _ words: String) -> ViewerLatestLine {
      ViewerLatestLine(id: "state-\(label)", words: words)
    }

    /// The footer's line while this window holds control.
    ///
    /// - Parameter words: What control's line says.
    /// - Returns: The line.
    static func viewerControlLine(_ words: String = "The agent waits") -> ViewerLatestLine {
      ViewerLatestLine(id: "control", words: words, tone: .link)
    }

    /// The sidebar while no host session is listed: All sessions alone.
    static let viewerNoSessions = ViewerSessionsState(icons: [], liveCount: 0, groups: [])

    /// The helper's row while two of its checks do not pass.
    static let viewerHelperSetup = ViewerHelperLine(
      tone: .attention,
      title: "Atlas needs setup",
      line: "2 checks don't pass",
      lineTone: .attention,
      menu: viewerHelperMenu(
        facts: viewerHelperFacts(
          ready: [
            "Mouse and keyboard: Not ready. Recover… starts them again.", "Screen Recording: Not allowed",
            "1 more check passes",
          ]
        ),
        logsOut: true
      )
    )

    /// The helper's row while it needs the person to recover it.
    ///
    /// - Parameter alertUp: Whether the recovery alert is up, rather than set aside with Not Now.
    /// - Returns: The row.
    static func viewerHelperNeedsYou(alertUp: Bool) -> ViewerHelperLine {
      ViewerHelperLine(
        tone: .attention,
        title: "Atlas needs you",
        line: "Recover the helper to go on",
        lineTone: .attention,
        menu: viewerHelperMenu(recovers: true, alertUp: alertUp)
      )
    }

    /// The helper's row in each of its states, for its previews and renders.
    @MainActor public static var viewerHelperLines: [PreviewState<ViewerHelperLine>] {
      [
        PreviewState(name: "Ready", value: viewerHelperReady(logsOut: true)),
        PreviewState(name: "Needs setup", value: viewerHelperSetup),
        PreviewState(name: "Needs you", value: viewerHelperNeedsYou(alertUp: false)),
        PreviewState(name: "Logged out", value: viewerLoggedOut.sidebar.helper),
        PreviewState(name: "Connecting", value: viewerConnecting.sidebar.helper),
      ]
    }

    /// The sidebar.
    ///
    /// - Parameters:
    ///   - sessions: The sessions; none listed by default.
    ///   - helper: The helper's health.
    /// - Returns: The sidebar.
    static func viewerSidebar(
      _ sessions: ViewerSessionsState = viewerNoSessions,
      helper: ViewerHelperLine
    ) -> ViewerSidebarState {
      ViewerSidebarState(sessions: sessions, helper: helper)
    }

    /// What a right click on the picture offers while this window holds no control.
    ///
    /// - Parameters:
    ///   - paused: Whether the picture is paused.
    ///   - actualSize: Whether the screen draws at actual size.
    /// - Returns: The menu's sections.
    static func viewerScreenMenu(paused: Bool = false, actualSize: Bool = false) -> [ViewerMenuSection] {
      [
        ViewerMenuSection([
          ViewerMenuItem("Copy Picture", command: .copyPicture),
          ViewerMenuItem("Save Picture…", command: .savePicture),
        ]),
        ViewerMenuSection([
          ViewerMenuItem("Actual Size", command: .toggleActualSize, isOn: actualSize),
          ViewerMenuItem(paused ? "Resume Picture" : "Pause Picture", command: .togglePause),
          ViewerMenuItem(
            "Picture Updates",
            children: [
              ViewerMenuItem("Live", command: .chooseRate(index: 0), isOn: true),
              ViewerMenuItem("Every 5 Seconds", command: .chooseRate(index: 1)),
            ]
          ),
        ]),
      ]
    }
  }
#endif
