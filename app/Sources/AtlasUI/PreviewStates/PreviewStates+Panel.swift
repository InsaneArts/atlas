#if DEBUG
  import SwiftUI

  extension PreviewStates {
    /// Every panel sample, in the order the previews list them.
    @MainActor package static var panel: [PreviewState<PanelState>] {
      [
        PreviewState(name: "Watching", value: panelWatching),
        PreviewState(name: "Agent working", value: panelAgentWorking),
        PreviewState(name: "In control", value: panelInControl),
        PreviewState(name: "Recovering", value: panelRecovering),
        PreviewState(name: "Input stopped", value: panelInputStopped),
        PreviewState(name: "Logged out", value: panelLoggedOut),
        PreviewState(name: "No picture", value: panelNoPicture),
        PreviewState(name: "Confirm log-out", value: panelConfirmLogOut),
        PreviewState(name: "Log-out held", value: panelLogOutHeld),
        PreviewState(name: "Working message", value: panelWorkingMessage),
        PreviewState(name: "Unavailable", value: panelUnavailable),
      ]
    }

    /// Nothing acts in the helper; its desktop shows, and every action is open.
    @MainActor static let panelWatching = PanelState(
      status: StatusSummary(label: "Watching", tone: .ready),
      reason: "Ready for agents",
      stage: SampleDesktop.picture.map(PanelStage.picture) ?? .placeholder("Waiting for the helper's screen…"),
      recovery: nil,
      facts: [
        Fact(name: "Helper", value: "Atlas"),
        Fact(name: "Display", value: "1512 × 982"),
        Fact(name: "Last action", value: "Agent click · 7 s ago"),
      ],
      isViewerPrimary: true,
      secondary: .switchToHelper(isEnabled: true),
      logOut: .none,
      feedback: nil,
      menu: PanelMenu(opensAtLogin: false, recover: .enabled, logOut: .enabled)
    )

    /// An agent acts in the helper: logging out waits.
    @MainActor static let panelAgentWorking: PanelState = {
      var state = panelWatching
      state.status = StatusSummary(label: "Agent working", tone: .active)
      state.reason = "An agent is acting in the helper"
      state.menu.logOut = .disabled
      return state
    }()

    /// A viewer holds control: the second button releases it.
    @MainActor static let panelInControl: PanelState = {
      var state = panelWatching
      state.status = StatusSummary(label: "Viewer has control", tone: .active)
      state.reason = "A viewer has control"
      state.secondary = .revokeControl
      return state
    }()

    /// The last action's outcome is unknown: the recovery card shows, and Recover leaves the menu.
    @MainActor static let panelRecovering: PanelState = {
      var state = panelWatching
      state.status = StatusSummary(label: "Needs you", tone: .attention)
      state.reason = nil
      state.recovery = .unfinished("An action's outcome is unknown. Recover the helper before anything else runs.")
      state.menu = PanelMenu(opensAtLogin: false, recover: .hidden, logOut: .disabled)
      return state
    }()

    /// The helper's input stopped after an account switch: the card offers Recover, and Recover leaves the menu.
    @MainActor static let panelInputStopped: PanelState = {
      var state = panelWatching
      state.status = StatusSummary(label: "Not ready", tone: .attention)
      state.reason = nil
      state.recovery = .notReady("The helper's input stopped. Recover starts it again.")
      state.menu.recover = .hidden
      return state
    }()

    /// The helper is logged out: Log In… takes the desktop's place, and Switch, Recover and Log Out leave.
    @MainActor static let panelLoggedOut: PanelState = {
      var state = panelWatching
      state.status = StatusSummary(label: "Logged out", tone: .quiet)
      state.reason = nil
      state.stage = .loggedOut(
        text: "Agents can't use it until you log in to it again. Then switch back here; Atlas carries on by itself.",
        isEnabled: true
      )
      state.isViewerPrimary = false
      state.secondary = nil
      state.menu = PanelMenu(opensAtLogin: false, recover: .hidden, logOut: .hidden)
      return state
    }()

    /// The last capture gave no picture.
    @MainActor static let panelNoPicture: PanelState = {
      var state = panelWatching
      state.stage = .placeholder("The helper's screen is not available")
      return state
    }()

    /// Log Out Helper… was chosen: the confirmation shows.
    @MainActor static let panelConfirmLogOut: PanelState = {
      var state = panelWatching
      state.logOut = .confirming(isEnabled: true)
      state.menu.logOut = .disabled
      return state
    }()

    /// An app held the last log-out up: Log Out Anyway is offered under what it said.
    @MainActor static let panelLogOutHeld: PanelState = {
      var state = panelWatching
      state.logOut = .held(isEnabled: true)
      state.feedback = Feedback(text: "The helper has not logged out yet", isWorking: false)
      return state
    }()

    /// A command runs: its words show beside a spinner, and every button that starts one waits.
    @MainActor static let panelWorkingMessage: PanelState = {
      var state = panelWatching
      state.secondary = .switchToHelper(isEnabled: false)
      state.feedback = Feedback(text: "Recovering the helper…", isWorking: true)
      state.menu = PanelMenu(opensAtLogin: false, recover: .disabled, logOut: .disabled)
      return state
    }()

    /// The service cannot reach the helper: no picture, and its facts unknown.
    @MainActor static let panelUnavailable: PanelState = {
      var state = panelWatching
      state.status = StatusSummary(label: "Unavailable", tone: .quiet)
      state.reason = "Connecting to the helper"
      state.stage = .placeholder("The helper's screen is not available")
      state.facts = [
        Fact(name: "Helper", value: "—"),
        Fact(name: "Display", value: "—"),
        Fact(name: "Last action", value: "No recent actions"),
      ]
      state.secondary = .switchToHelper(isEnabled: false)
      state.menu.logOut = .disabled
      return state
    }()
  }
#endif
