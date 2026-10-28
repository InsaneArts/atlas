#if DEBUG
  import CoreGraphics

  extension PreviewStates {
    /// The standard size for main window samples, so renders compare at the same dimensions.
    public static let viewerSize = CGSize(width: 1440, height: 900)
    /// 860: the least width the main window shrinks to, where its top row takes its narrowest layout.
    public static let viewerNarrowest: CGFloat = 860

    /// Every main window sample, in the order the previews list them: each top-row state, each alert and each empty
    /// message, then each tab's pane, then the host sessions' states.
    @MainActor public static var viewer: [PreviewState<ViewerState>] {
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
        PreviewState(name: "Sessions", value: viewerSessions),
        PreviewState(name: "Sessions, menu open", value: viewerSessionsMenuOpen),
        PreviewState(name: "Activity by session", value: viewerActivityBySession),
        PreviewState(name: "Activity, needs you", value: viewerActivityNeedsYou),
        PreviewState(name: "Activity, one session", value: viewerActivityOneSession),
        PreviewState(name: "Session detail", value: viewerSessionDetail),
        PreviewState(name: "Focus changed", value: viewerFocusChanged),
      ]
    }

    /// The tabs, one of them selected.
    ///
    /// - Parameter selected: The tab shown.
    /// - Returns: The tabs.
    static func viewerTabs(_ selected: ViewerTab = .screen) -> CapsuleTabsState<ViewerTab> {
      CapsuleTabsState(
        tabs: [
          CapsuleTab(id: .screen, title: "Screen", symbol: .display, help: "Screen (⌘1)"),
          CapsuleTab(id: .activity, title: "Activity", symbol: .list, help: "Activity (⌘2)"),
          CapsuleTab(id: .apps, title: "Apps", symbol: .windows, help: "Apps (⌘3)"),
          CapsuleTab(id: .screenshots, title: "Screenshots", symbol: .photos, help: "Screenshots (⌘4)"),
        ],
        selected: selected,
        label: "View"
      )
    }

    /// The pause button.
    ///
    /// - Parameter paused: Whether the picture is paused.
    /// - Returns: The toggle.
    static func viewerPause(paused: Bool = false) -> ViewerToggle {
      paused
        ? ViewerToggle(isOn: true, label: "Resume Picture", help: "Resume Picture (⇧⌘P)")
        : ViewerToggle(isOn: false, label: "Pause Picture", help: "Pause Picture (⇧⌘P)")
    }

    /// The Details button.
    ///
    /// - Parameter shown: Whether Details shows.
    /// - Returns: The toggle.
    static func viewerDetailsToggle(shown: Bool = false) -> ViewerToggle {
      shown
        ? ViewerToggle(isOn: true, label: "Hide Details", help: "Hide Details (⌥⌘I)")
        : ViewerToggle(isOn: false, label: "Show Details", help: "Show Details (⌥⌘I)")
    }

    /// Take Control, as offered.
    ///
    /// - Parameter isEnabled: Whether it can be taken now.
    /// - Returns: The button.
    static func viewerTakeControl(isEnabled: Bool) -> ViewerControlButton {
      ViewerControlButton(
        title: "Take Control",
        isEnabled: isEnabled,
        help: "Take Control (⌃⇧⎋)",
        event: .takeControl,
        titles: viewerControlTitles
      )
    }

    /// Every title the white capsule can show, which it keeps the widest of.
    static let viewerControlTitles = ["Take Control", "Give Back", "Connecting…", "Recover…"]

    /// The top row's buttons.
    ///
    /// - Parameters:
    ///   - keys: Keys; hidden by default.
    ///   - agent: Stop or Resume.
    ///   - control: The white capsule; nil hides it.
    ///   - paused: Whether the picture is paused.
    ///   - showsPause: Whether there is a picture to pause.
    ///   - refusals: Take Control's refusals counted.
    ///   - details: Whether Details shows.
    ///   - showsMore: Whether there is a picture for ⋯ to act on.
    /// - Returns: The buttons.
    static func viewerActions(
      keys: ViewerKeysButton = .hidden,
      agent: ViewerAgentButton? = nil,
      control: ViewerControlButton?,
      paused: Bool = false,
      showsPause: Bool = true,
      refusals: Int = 0,
      details: Bool = false,
      showsMore: Bool = true
    ) -> ViewerTopActions {
      ViewerTopActions(
        pause: showsPause ? viewerPause(paused: paused) : nil,
        keys: keys,
        agent: agent,
        agentHelp: agentHelp(agent),
        control: control,
        refusals: refusals,
        more: showsMore ? ViewerToggle(isOn: false, label: "More", help: "More") : nil,
        details: viewerDetailsToggle(shown: details)
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

    /// The top row.
    ///
    /// - Parameters:
    ///   - status: The status.
    ///   - actions: The buttons.
    ///   - tab: The tab shown.
    ///   - unseen: What Activity holds unseen.
    ///   - isInControl: Whether this window holds control.
    /// - Returns: The top row.
    static func viewerTop(
      _ status: StatusSummary,
      _ actions: ViewerTopActions,
      tab: ViewerTab = .screen,
      unseen: ViewerUnseen = .none,
      isInControl: Bool = false
    ) -> ViewerTopRowState {
      ViewerTopRowState(
        status: status,
        tabs: viewerTabs(tab),
        activityUnseen: unseen,
        actions: actions,
        isInControl: isInControl,
        isFullScreen: false
      )
    }
  }
#endif
