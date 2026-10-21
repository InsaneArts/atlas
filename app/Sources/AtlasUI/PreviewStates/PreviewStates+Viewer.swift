#if DEBUG
  import CoreGraphics

  extension PreviewStates {
    /// The size the viewer's samples are drawn at: a window on a laptop's screen.
    package static let viewerSize = CGSize(width: 1280, height: 820)

    /// Every viewer sample, in the order the previews list them.
    @MainActor package static var viewer: [PreviewState<ViewerState>] {
      [
        PreviewState(name: "Watching", value: viewerWatching),
        PreviewState(name: "Sidebar", value: viewerSidebar),
        PreviewState(name: "Sidebar and control", value: viewerSidebarControl),
        PreviewState(name: "Control", value: viewerControl),
        PreviewState(name: "Recovery", value: viewerRecovery),
        PreviewState(name: "Paused", value: viewerPaused),
        PreviewState(name: "Paused before a picture", value: viewerPausedBeforePicture),
        PreviewState(name: "Unavailable", value: viewerUnavailable),
        PreviewState(name: "Notice", value: viewerNotice),
        PreviewState(name: "Notice, long", value: viewerNoticeLong),
        PreviewState(name: "Logged out", value: viewerLoggedOut),
        PreviewState(name: "Connecting", value: viewerConnecting),
        PreviewState(name: "Typing", value: viewerTyping),
      ]
    }

    /// The two latest actions an agent took, oldest first, as captions show them.
    private static let agentCaptions = [
      ActivityEntry(id: 1, initial: "A", line: "Agent ran a command", when: "7 s ago", kind: .others, isNewest: false),
      ActivityEntry(id: 0, initial: "A", line: "Agent sent 2 actions", when: "just now", kind: .others, isNewest: true),
    ]

    /// Recent activity, newest first: an agent's two actions, then the person's.
    private static let recentActivity = [
      ActivityEntry(id: 0, initial: "A", line: "Agent sent 2 actions", when: "just now", kind: .others, isNewest: true),
      ActivityEntry(id: 1, initial: "A", line: "Agent ran a command", when: "7 s ago", kind: .others, isNewest: false),
      ActivityEntry(id: 2, initial: "Y", line: "You sent 5 actions", when: "4 min ago", kind: .yours, isNewest: false),
    ]

    /// Every check ready; native dialogs not checked yet.
    private static let readyChecks = [
      ReadinessItem(name: "Helper", text: "Ready", state: .ready),
      ReadinessItem(name: "Mouse and keyboard", text: "Ready", state: .ready),
      ReadinessItem(name: "Save and Open dialogs", text: "Not checked yet", state: .unknown),
    ]

    /// The session's facts, with control as `control` reads.
    ///
    /// - Parameters:
    ///   - control: Whether this window could take control, and if not, why.
    ///   - pending: Actions of unknown outcome.
    /// - Returns: The facts.
    private static func facts(control: String, pending: String = "None") -> [Fact] {
      [
        Fact(name: "Display", value: "1512 × 982"),
        Fact(name: "Input", value: "Ready"),
        Fact(name: "Control", value: control),
        Fact(name: "Pending actions", value: pending),
      ]
    }

    /// The sidebar's toggle.
    ///
    /// - Parameter shown: Whether the person asked for the sidebar.
    /// - Returns: The toggle.
    static func viewerSidebarToggle(shown: Bool) -> ViewerToggle {
      shown
        ? ViewerToggle(isOn: true, label: "Hide sidebar", help: "Hide Sidebar (⌥⌘I)")
        : ViewerToggle(isOn: false, label: "Show sidebar", help: "Show Sidebar (⌥⌘I)")
    }

    /// The pause button.
    ///
    /// - Parameter paused: Whether the picture is paused.
    /// - Returns: The toggle.
    static func viewerPauseToggle(paused: Bool) -> ViewerToggle {
      paused
        ? ViewerToggle(isOn: true, label: "Resume picture", help: "Resume Picture (⇧⌘P)")
        : ViewerToggle(isOn: false, label: "Pause picture", help: "Pause Picture (⇧⌘P)")
    }

    /// Take Control, as offered.
    ///
    /// - Parameter isEnabled: Whether it can be taken now.
    /// - Returns: The button.
    private static func takeControl(isEnabled: Bool) -> ViewerControlButton {
      ViewerControlButton(
        title: "Take Control",
        isEnabled: isEnabled,
        help: "Use your mouse and keyboard in the helper (⌃⇧⎋)",
        event: .takeControl
      )
    }

    /// An agent acts in the helper while the person watches.
    @MainActor package static let viewerWatching = ViewerState(
      header: ViewerHeaderState(
        status: StatusSummary(label: "Agent working", tone: .active),
        isWorking: true,
        detail: "Atlas · Live",
        sidebar: viewerSidebarToggle(shown: false),
        isFullScreen: false
      ),
      stage: ViewerStageState(
        picture: SampleDesktop.picture,
        isDimmed: false,
        isInControl: false,
        placeholder: nil,
        chip: nil,
        notice: nil,
        captions: agentCaptions
      ),
      recovery: nil,
      footer: ViewerFooterState(
        hint: "An agent is acting in the helper.",
        offersRecovery: false,
        pause: viewerPauseToggle(paused: false),
        keysEnabled: false,
        control: takeControl(isEnabled: false)
      ),
      sidebar: ViewerSidebarState(
        account: "Atlas",
        reason: "An agent is acting in the helper",
        recent: recentActivity,
        ready: readyChecks,
        facts: facts(control: "Agent working"),
        rate: .everyTwoSeconds,
        pictureNote: "1512 × 982 pixels. Pictures of the helper stay in memory and are never saved."
      )
    )

    /// Nothing acts in the helper; control is available.
    @MainActor private static let viewerObserving: ViewerState = {
      var state = viewerWatching
      state.header.status = StatusSummary(label: "Watching", tone: .ready)
      state.header.isWorking = false
      state.footer.hint = "Watching. Control-Shift-Escape takes control."
      state.footer.control = takeControl(isEnabled: true)
      state.sidebar.facts = facts(control: "Available")
      return state
    }()

    /// Watching, with the sidebar open: the captions step aside once the window is wide enough to show it.
    @MainActor package static let viewerSidebar: ViewerState = {
      var state = viewerObserving
      state.header.sidebar = viewerSidebarToggle(shown: true)
      state.sidebar.reason = "Watching the helper"
      return state
    }()

    /// The person holds control.
    @MainActor package static let viewerControl: ViewerState = {
      var state = viewerWatching
      state.header.status = StatusSummary(label: "You have control", tone: .active)
      state.header.isWorking = false
      state.stage.isInControl = true
      state.stage.chip = .inControl
      state.stage.captions = []
      state.footer.hint = "Click the helper's screen to connect your keyboard."
      state.footer.keysEnabled = true
      state.footer.control = ViewerControlButton(
        title: "Give Back",
        isEnabled: true,
        help: "Give control back (⌃⇧⎋)",
        event: .giveBack
      )
      state.sidebar.reason = "You have mouse and keyboard control"
      state.sidebar.facts = facts(control: "This window")
      return state
    }()

    /// The person holds control, with the sidebar open.
    @MainActor package static let viewerSidebarControl: ViewerState = {
      var state = viewerControl
      state.header.sidebar = viewerSidebarToggle(shown: true)
      return state
    }()

    /// The last action's outcome is unknown: the recovery alert shows.
    @MainActor package static let viewerRecovery: ViewerState = {
      var state = viewerWatching
      let failed = ActivityEntry(
        id: 0,
        initial: "A",
        line: "Agent couldn't send 2 actions",
        when: "just now",
        kind: .failed,
        isNewest: true
      )
      state.header.status = StatusSummary(label: "Needs you", tone: .attention)
      state.header.isWorking = false
      state.stage.captions = [agentCaptions[0], failed]
      state.recovery = ViewerRecoveryPrompt(
        title: "The last action's outcome is unknown",
        reason: "Atlas can't tell whether the last action worked.",
        isBusy: false,
        recoverTitle: "Recover"
      )
      state.footer.hint = "Recover before taking control."
      state.sidebar.reason = "Atlas can't tell whether the last action worked."
      state.sidebar.recent = [failed] + recentActivity.dropFirst()
      state.sidebar.ready[1] = ReadinessItem(
        name: "Mouse and keyboard",
        text: "Mouse and keyboard aren't ready.",
        state: .notReady
      )
      state.sidebar.facts = facts(control: "After recovery", pending: "1")
      return state
    }()

    /// The picture is paused; work in the helper goes on.
    @MainActor package static let viewerPaused: ViewerState = {
      var state = viewerObserving
      state.header.status = StatusSummary(label: "Paused", tone: .quiet)
      state.header.detail = "Atlas · Picture paused"
      state.stage.isDimmed = true
      state.stage.chip = .paused
      state.stage.captions = []
      state.footer.hint = "Picture paused. Work in the helper continues."
      state.footer.pause = viewerPauseToggle(paused: true)
      state.footer.control = takeControl(isEnabled: false)
      state.sidebar.reason = "An agent is acting in the helper"
      return state
    }()

    /// The picture was paused before the first one arrived: the stage says so in place of a picture.
    @MainActor package static let viewerPausedBeforePicture: ViewerState = {
      var state = viewerPaused
      state.stage.picture = nil
      state.stage.placeholder = .paused
      state.stage.chip = nil
      state.sidebar.pictureNote = "Pictures of the helper stay in memory and are never saved."
      return state
    }()

    /// The helper does not answer, so there is no picture.
    @MainActor package static let viewerUnavailable: ViewerState = {
      var state = viewerWatching
      state.header.status = StatusSummary(label: "Reconnecting", tone: .quiet)
      state.header.isWorking = false
      state.header.detail = "Atlas · No picture"
      state.stage.picture = nil
      state.stage.placeholder = .unavailable(
        reason: "Atlas isn't answering in the helper. Open Getting Started if this lasts."
      )
      state.footer.hint = "Control returns once the helper's screen is back."
      state.sidebar.reason = "Atlas isn't answering in the helper"
      state.sidebar.facts = facts(control: "Unavailable")
      state.sidebar.pictureNote = "Pictures of the helper stay in memory and are never saved."
      return state
    }()

    /// A line about what just happened shows over the picture.
    @MainActor package static let viewerNotice: ViewerState = {
      var state = viewerObserving
      state.stage.notice = "Control released."
      state.sidebar.reason = "An agent is acting in the helper"
      return state
    }()

    /// A notice longer than one line wraps over the picture instead of being cut short.
    @MainActor package static let viewerNoticeLong: ViewerState = {
      var state = viewerNotice
      state.stage.notice =
        "Control ended because the helper's screen changed size. Take control again once the picture settles, and nothing you sent is lost."
      return state
    }()

    /// The helper is logged out: logging in to it is the way back.
    @MainActor package static let viewerLoggedOut: ViewerState = {
      var state = viewerUnavailable
      state.header.status = StatusSummary(label: "Logged out", tone: .quiet)
      state.stage.placeholder = .loggedOut(
        text: "Agents can't use it until you log in to it again. Then switch back here; Atlas carries on by itself.",
        canLogIn: true
      )
      state.sidebar.reason = "The helper is logged out, so agents can't use it"
      state.sidebar.facts = facts(control: "Logged out")
      return state
    }()

    /// The window has just opened: no reading has arrived yet.
    @MainActor package static let viewerConnecting = ViewerState(
      header: ViewerHeaderState(
        status: StatusSummary(label: "Connecting", tone: .quiet),
        isWorking: false,
        detail: "Helper · Waiting for the first picture",
        sidebar: viewerSidebarToggle(shown: false),
        isFullScreen: false
      ),
      stage: ViewerStageState(
        picture: nil,
        isDimmed: false,
        isInControl: false,
        placeholder: .connecting,
        chip: nil,
        notice: nil,
        captions: []
      ),
      recovery: nil,
      footer: ViewerFooterState(
        hint: "Watching. Control-Shift-Escape takes control.",
        offersRecovery: false,
        pause: viewerPauseToggle(paused: false),
        keysEnabled: false,
        control: takeControl(isEnabled: false)
      ),
      sidebar: ViewerSidebarState(
        account: "Helper",
        reason: "Connecting to the helper",
        recent: [],
        ready: [],
        facts: [
          Fact(name: "Display", value: "—"),
          Fact(name: "Input", value: "Unavailable"),
          Fact(name: "Control", value: "Unavailable"),
          Fact(name: "Pending actions", value: "None"),
        ],
        rate: .everyTwoSeconds,
        pictureNote: "Pictures of the helper stay in memory and are never saved."
      )
    )

    /// The person types through an input method; the composed text shows before it is sent.
    @MainActor package static let viewerTyping: ViewerState = {
      var state = viewerControl
      state.stage.chip = .typing("こんにちは")
      return state
    }()
  }
#endif
