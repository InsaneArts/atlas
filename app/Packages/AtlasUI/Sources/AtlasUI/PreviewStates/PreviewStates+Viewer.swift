#if DEBUG
  import CoreGraphics

  extension PreviewStates {
    /// The size the main window's samples are drawn at: the boards' window, so renders compare with them one to one.
    public static let viewerSize = CGSize(width: 1440, height: 900)

    /// Every main window sample, in the order the previews list them: each top-row state, each alert and each empty
    /// message, then each tab's pane.
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
  }
#endif
