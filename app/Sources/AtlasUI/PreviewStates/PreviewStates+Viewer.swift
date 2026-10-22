#if DEBUG
  import CoreGraphics

  extension PreviewStates {
    /// The size the main window's samples are drawn at: the boards' window, so renders compare with them one to one.
    package static let viewerSize = CGSize(width: 1440, height: 900)

    /// Every main window sample, in the order the previews list them: each top-row state, each alert and each empty
    /// message, then each tab's pane.
    @MainActor package static var viewer: [PreviewState<ViewerState>] {
      [
        PreviewState(name: "Working", value: viewerWorking),
        PreviewState(name: "Idle", value: viewerIdle),
        PreviewState(name: "You have control", value: viewerControl),
        PreviewState(name: "Needs you", value: viewerNeedsYou),
        PreviewState(name: "Stopped", value: viewerStopped),
        PreviewState(name: "In use elsewhere", value: viewerInUseElsewhere),
        PreviewState(name: "Paused", value: viewerPaused),
        PreviewState(name: "Details", value: viewerDetails),
        PreviewState(name: "Details, some checks not passing", value: viewerDetailsChecks),
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
        PreviewState(name: "Apps", value: viewerApps),
        PreviewState(name: "Apps, paused", value: viewerAppsPaused),
        PreviewState(name: "Show on screen", value: viewerShowOnScreen),
        PreviewState(name: "Show on screen, at the right edge", value: viewerShowOnScreenRightEdge),
        PreviewState(name: "Screenshots", value: viewerShots),
        PreviewState(name: "A screenshot, large", value: viewerShotLarge),
        PreviewState(name: "Screenshots, none yet", value: viewerShotsEmpty),
      ]
    }

    /// The tabs, one of them selected.
    ///
    /// - Parameter selected: The tab shown.
    /// - Returns: The tabs.
    static func viewerTabs(_ selected: ViewerTab = .screen) -> CapsuleTabsState<ViewerTab> {
      CapsuleTabsState(
        tabs: [
          CapsuleTab(id: .screen, title: "Screen"),
          CapsuleTab(id: .activity, title: "Activity"),
          CapsuleTab(id: .apps, title: "Apps"),
          CapsuleTab(id: .screenshots, title: "Screenshots"),
        ],
        selected: selected,
        label: "What you see"
      )
    }

    /// The pause button.
    ///
    /// - Parameter paused: Whether the picture is paused.
    /// - Returns: The toggle.
    static func viewerPause(paused: Bool = false) -> ViewerToggle {
      paused
        ? ViewerToggle(isOn: true, label: "Resume picture", help: "Resume Picture (⇧⌘P)")
        : ViewerToggle(isOn: false, label: "Pause picture", help: "Pause Picture (⇧⌘P)")
    }

    /// The Details button.
    ///
    /// - Parameter shown: Whether Details shows.
    /// - Returns: The toggle.
    static func viewerDetailsToggle(shown: Bool = false) -> ViewerToggle {
      shown
        ? ViewerToggle(isOn: true, label: "Hide details", help: "Hide Details (⌥⌘I)")
        : ViewerToggle(isOn: false, label: "Show details", help: "Show Details (⌥⌘I)")
    }

    /// Take Control, as offered.
    ///
    /// - Parameter isEnabled: Whether it can be taken now.
    /// - Returns: The button.
    static func viewerTakeControl(isEnabled: Bool) -> ViewerControlButton {
      ViewerControlButton(
        title: "Take Control",
        shortTitle: "Take",
        isEnabled: isEnabled,
        help: "Use your mouse and keyboard in the helper (⌃⇧⎋)",
        event: .takeControl
      )
    }

    /// The top row's buttons.
    ///
    /// - Parameters:
    ///   - keys: Keys.
    ///   - agent: Stop or Resume.
    ///   - control: The white capsule.
    ///   - paused: Whether the picture is paused.
    ///   - details: Whether Details shows.
    /// - Returns: The buttons.
    static func viewerActions(
      keys: ViewerKeysButton = .disabled,
      agent: ViewerAgentButton? = nil,
      control: ViewerControlButton,
      paused: Bool = false,
      details: Bool = false
    ) -> ViewerTopActions {
      ViewerTopActions(
        pause: viewerPause(paused: paused),
        keys: keys,
        agent: agent,
        agentHelp: agentHelp(agent),
        control: control,
        more: ViewerToggle(isOn: false, label: "More", help: "The picture and the helper"),
        details: viewerDetailsToggle(shown: details)
      )
    }

    /// Stop's or Resume's tooltip.
    ///
    /// - Parameter agent: Stop or Resume.
    /// - Returns: Its tooltip, with its shortcut; empty for neither.
    private static func agentHelp(_ agent: ViewerAgentButton?) -> String {
      switch agent {
      case .stop: "Stop what the agent is doing in the helper, until you resume (⌘.)"
      case .resume: "Let agents act in the helper again (⌘.)"
      case nil: ""
      }
    }

    /// The top row.
    ///
    /// - Parameters:
    ///   - status: The status.
    ///   - actions: The buttons.
    ///   - tab: The tab shown.
    /// - Returns: The top row.
    static func viewerTop(
      _ status: StatusSummary,
      _ actions: ViewerTopActions,
      tab: ViewerTab = .screen
    ) -> ViewerTopRowState {
      ViewerTopRowState(status: status, tabs: viewerTabs(tab), actions: actions, isFullScreen: false)
    }

    /// An agent's three latest actions, oldest first, as captions show them (mac.py `WORKING`).
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

    /// An agent acts in the helper while the person watches (MacWorking).
    @MainActor package static let viewerWorking = ViewerState(
      top: viewerTop(
        StatusSummary(label: "Working", tone: .active, mark: .pulsing, line: "Turning Show Center off in Opie"),
        viewerActions(agent: .stop, control: viewerTakeControl(isEnabled: true))
      ),
      screen: ViewerScreenState(picture: SampleDesktop.picture, captions: workingCaptions)
    )

    /// Nothing acts in the helper; control is there to take.
    @MainActor package static let viewerIdle = ViewerState(
      top: viewerTop(
        StatusSummary(label: "Idle", tone: .quiet, mark: .dot, line: "Last: turned Show Center off · 2 min ago"),
        viewerActions(control: viewerTakeControl(isEnabled: true))
      ),
      screen: ViewerScreenState(picture: SampleDesktop.picture)
    )

    /// The person holds control (MacControl).
    @MainActor package static let viewerControl = ViewerState(
      top: viewerTop(
        StatusSummary(label: "You have control", tone: .link, mark: .dot, line: "The agent waits"),
        viewerActions(
          keys: .pressed,
          control: ViewerControlButton(
            title: "Give Back",
            isEnabled: true,
            help: "Give control back (⌃⇧⎋)",
            event: .giveBack
          )
        )
      ),
      screen: ViewerScreenState(picture: SampleDesktop.picture, look: .inControl, chip: .inControl)
    )

    /// An action may not have finished: the picture dims under the recovery alert (MacRecover).
    @MainActor package static let viewerNeedsYou = ViewerState(
      top: viewerTop(
        StatusSummary(label: "Needs you", tone: .attention, mark: .dot, line: "An action may not have finished"),
        viewerActions(
          keys: .hidden,
          control: ViewerControlButton(
            title: "Recover…",
            isEnabled: true,
            help: "Check the helper and start its mouse and keyboard again",
            event: .askRecover
          )
        )
      ),
      screen: ViewerScreenState(picture: SampleDesktop.picture, look: .dimmed),
      alert: OrbitAlertState(
        title: "Recover the helper?",
        message:
          "An action may not have finished. Atlas checks the helper and starts its mouse and keyboard again.",
        buttons: [
          OrbitAlertButton(id: .confirm, title: "Recover", role: .action),
          OrbitAlertButton(id: .cancel, title: "Not Now", role: .cancel),
        ]
      )
    )

    /// The person stopped agents: Resume stands where Stop was.
    @MainActor package static let viewerStopped = ViewerState(
      top: viewerTop(
        StatusSummary(label: "Stopped", tone: .attention, mark: .square, line: "The agent waits for you"),
        viewerActions(agent: .resume(isEnabled: true), control: viewerTakeControl(isEnabled: true))
      ),
      screen: ViewerScreenState(picture: SampleDesktop.picture)
    )

    /// Another window holds control, so Take Control waits.
    @MainActor package static let viewerInUseElsewhere = ViewerState(
      top: viewerTop(
        StatusSummary(label: "In use elsewhere", tone: .quiet, mark: .dot, line: "Another window has control"),
        viewerActions(control: viewerTakeControl(isEnabled: false))
      ),
      screen: ViewerScreenState(picture: SampleDesktop.picture)
    )

    /// The picture is paused; work in the helper goes on (MacEmpty).
    @MainActor package static let viewerPaused = ViewerState(
      top: viewerTop(
        StatusSummary(label: "Idle", tone: .quiet, mark: .dot, line: "Picture paused"),
        viewerActions(control: viewerTakeControl(isEnabled: false), paused: true)
      ),
      screen: ViewerScreenState(picture: SampleDesktop.picture, look: .paused, chip: .paused)
    )

    /// Idle, with Details beside the screen (MacDetails).
    @MainActor package static let viewerDetails: ViewerState = {
      var state = viewerIdle
      state.top.actions.details = viewerDetailsToggle(shown: true)
      state.details = ViewerDetailsState(
        helper: [
          Fact(name: "Account", value: "Atlas"),
          Fact(name: "State", value: "Ready"),
          Fact(name: "Display", value: "1440 × 900"),
        ],
        ready: [ViewerReadyRow(id: "all", name: "Ready to act", value: "All checks pass", mark: .ready)],
        updates: "Live",
        newest: "just now",
        session: [
          Fact(name: "Logged in", value: "09:12"),
          Fact(name: "Agents", value: "Claude Code, Codex"),
        ]
      )
      return state
    }()

    /// Details while one check fails and one is not checked yet: only those two list, with the count of the rest.
    @MainActor package static let viewerDetailsChecks: ViewerState = {
      var state = viewerDetails
      state.details?.ready = [
        ViewerReadyRow(
          id: "Mouse and keyboard",
          name: "Mouse and keyboard",
          mark: .notReady,
          note: "Mouse and keyboard aren't ready."
        ),
        ViewerReadyRow(
          id: "Save and Open dialogs",
          name: "Save and Open dialogs",
          mark: .unknown,
          note: "Not checked yet"
        ),
        ViewerReadyRow(id: "others", name: "1 other passes", isQuiet: true),
      ]
      return state
    }()

    /// The helper doesn't answer, so there is no picture: Recover is the way back (MacEmpty).
    @MainActor package static let viewerNotReady = ViewerState(
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
    @MainActor package static let viewerLoggedOut = ViewerState(
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
    @MainActor package static let viewerConnecting = ViewerState(
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

    /// Switch to the Helper… asks first, over a logged-out helper.
    @MainActor package static let viewerSwitch: ViewerState = {
      var state = viewerLoggedOut
      state.alert = OrbitAlertState(
        title: "Switch to the helper?",
        message:
          "Your screen locks so you can log in to the helper. Everything here keeps running, and you can switch back any time.",
        buttons: [
          OrbitAlertButton(id: .confirm, title: "Switch", role: .action),
          OrbitAlertButton(id: .cancel, title: "Cancel", role: .cancel),
        ]
      )
      return state
    }()

    /// Log Out the Helper… asks first: Cancel in blue, Log Out in red.
    @MainActor package static let viewerLogOut: ViewerState = {
      var state = viewerIdle
      state.alert = OrbitAlertState(
        title: "Log out the helper?",
        message: "Its apps quit, and agents can't use it until you log in to it again.",
        buttons: [
          OrbitAlertButton(id: .cancel, title: "Cancel", role: .safeCancel),
          OrbitAlertButton(id: .confirm, title: "Log Out", role: .destructive),
        ]
      )
      return state
    }()

    /// An app held the log-out up: Log Out Anyway is offered.
    @MainActor package static let viewerLogOutAnyway: ViewerState = {
      var state = viewerIdle
      state.alert = OrbitAlertState(
        title: "The helper didn't log out",
        message:
          "An app in the helper is holding it up. Log Out Anyway quits it, and anything unsaved there is lost.",
        buttons: [
          OrbitAlertButton(id: .cancel, title: "Cancel", role: .safeCancel),
          OrbitAlertButton(id: .confirm, title: "Log Out Anyway", role: .destructive),
        ]
      )
      return state
    }()

    /// Restart the Helper… asks first.
    @MainActor package static let viewerRestart: ViewerState = {
      var state = viewerIdle
      state.alert = OrbitAlertState(
        title: "Restart the helper?",
        message: "Atlas starts its service in the helper again. Anything an agent is doing there stops.",
        buttons: [
          OrbitAlertButton(id: .confirm, title: "Restart", role: .action),
          OrbitAlertButton(id: .cancel, title: "Cancel", role: .cancel),
        ]
      )
      return state
    }()

    /// The person types through an input method in control; the composed text shows before it is sent.
    @MainActor package static let viewerTyping: ViewerState = {
      var state = viewerControl
      state.screen.chip = .typing("こんにちは")
      return state
    }()

    /// Take Control was still refused after its one retry, as an agent's action ran.
    @MainActor package static let viewerMidAction: ViewerState = {
      var state = viewerWorking
      state.screen.notice = "The agent is mid-action. Try again in a moment."
      return state
    }()

    /// The paused picture at actual size, one view point a display point: it fades as the fitted one does.
    @MainActor package static let viewerActualSizePaused: ViewerState = {
      var state = viewerPaused
      state.screen.actualSize = CGSize(width: 1440, height: 900)
      return state
    }()

    /// The helper needs the person, at actual size: the picture dims under the alert as the fitted one does.
    @MainActor package static let viewerActualSizeNeedsYou: ViewerState = {
      var state = viewerNeedsYou
      state.screen.actualSize = CGSize(width: 1440, height: 900)
      return state
    }()
  }
#endif
