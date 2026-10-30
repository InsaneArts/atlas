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
        PreviewState(name: "Activity, a moment opened", value: viewerActivityOpened),
        PreviewState(name: "Activity, problems", value: viewerActivityProblems),
        PreviewState(name: "Activity, days before", value: viewerActivityDays),
        PreviewState(name: "Activity, many moments", value: viewerActivityMany),
        PreviewState(name: "Apps", value: viewerApps),
        PreviewState(name: "Apps, paused", value: viewerAppsPaused),
        PreviewState(name: "Apps, one session", value: viewerAppsOneSession),
        PreviewState(name: "Apps, one session, none yet", value: viewerAppsOneSessionEmpty),
        PreviewState(name: "Apps, a copy", value: viewerAppsCopy),
        PreviewState(name: "Apps, hidden", value: viewerAppsHidden),
        PreviewState(name: "Apps, many", value: viewerAppsMany),
        PreviewState(name: "Show on screen", value: viewerShowOnScreen),
        PreviewState(name: "Show on screen, at the right edge", value: viewerShowOnScreenRightEdge),
        PreviewState(name: "Screenshots", value: viewerShots),
        PreviewState(name: "A screenshot, large", value: viewerShotLarge),
        PreviewState(name: "Screenshots, none yet", value: viewerShotsEmpty),
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
        PreviewState(name: "Screenshots, one session", value: viewerShotsOneSession),
        PreviewState(name: "Screenshots, one session, none yet", value: viewerShotsOneSessionEmpty),
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

    /// The Library's rows, one of them chosen while its list shows, each with how many its list holds once read.
    ///
    /// - Parameters:
    ///   - place: The place shown; the Screen by default, which chooses none.
    ///   - unseen: What Activity holds unseen; nothing by default.
    ///   - counts: How many each list holds, by its place; none read by default.
    /// - Returns: The rows.
    static func viewerLibrary(
      _ place: ViewerPlace = .screen,
      unseen: ViewerUnseen = .none,
      counts: [ViewerPlace: Int] = [:]
    ) -> [ViewerLibraryRow] {
      let rows = [
        ViewerLibraryRow(
          place: .activity,
          title: "Activity",
          help: "Activity (⌘2)",
          unseen: unseen,
          isChosen: place == .activity
        ),
        ViewerLibraryRow(
          place: .screenshots,
          title: "Screenshots",
          help: "Screenshots (⌘3)",
          isChosen: place == .screenshots
        ),
        ViewerLibraryRow(place: .apps, title: "Apps", help: "Apps (⌘4)", isChosen: place == .apps),
      ]
      return rows.map { row in
        var counted = row
        counted.count = counts[row.place]
        return counted
      }
    }

    /// The sidebar while no host session is listed: All sessions alone.
    static let viewerNoSessions = ViewerSessionsState(icons: [], liveCount: 0, groups: [])

    /// The helper's line while two of its checks do not pass.
    static let viewerHelperSetup = ViewerHelperLine(
      tone: .attention,
      words: "Helper needs setup · 2 checks",
      help: viewerHelperHelp(
        ready: [
          "Mouse and keyboard: Not ready. Recover… starts them again.", "Screen Recording: Not allowed",
          "1 more check passes",
        ]
      ),
      menu: viewerHelperMenu(logsOut: true)
    )

    /// The helper's line while it needs the person to recover it.
    ///
    /// - Parameter alertUp: Whether the recovery alert is up, rather than set aside with Not Now.
    /// - Returns: The line.
    static func viewerHelperNeedsYou(alertUp: Bool) -> ViewerHelperLine {
      ViewerHelperLine(
        tone: .attention,
        words: "Helper needs you",
        help: viewerHelperHelp(),
        menu: viewerHelperMenu(recovers: true, alertUp: alertUp)
      )
    }

    /// The sidebar.
    ///
    /// - Parameters:
    ///   - sessions: The sessions; none listed by default.
    ///   - place: The place shown, whose Library row is chosen; the Screen by default.
    ///   - unseen: What Activity holds unseen; nothing by default.
    ///   - helper: The helper's health.
    /// - Returns: The sidebar.
    static func viewerSidebar(
      _ sessions: ViewerSessionsState = viewerNoSessions,
      place: ViewerPlace = .screen,
      unseen: ViewerUnseen = .none,
      helper: ViewerHelperLine
    ) -> ViewerSidebarState {
      ViewerSidebarState(sessions: sessions, library: viewerLibrary(place, unseen: unseen), helper: helper)
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
