#if DEBUG
  extension PreviewStates {
    /// The Helper pane: the helper idle at 2560 × 1440, every check ready, every button free.
    static let settingsHelperPane = SettingsHelperState(
      account: SettingsHelperAccount(
        name: "Atlas",
        line: "The hidden helper login · 2560 × 1440",
        status: StatusSummary(label: "Idle", tone: .quiet, mark: .dot)
      ),
      ready: [
        SettingsReadyRow(item: ReadinessItem(name: "Helper is running", text: "", state: .ready)),
        SettingsReadyRow(item: ReadinessItem(name: "Mouse and keyboard", text: "", state: .ready), index: 1),
        SettingsReadyRow(item: ReadinessItem(name: "Save and Open dialogs", text: "", state: .ready), index: 2),
      ],
      buttons: SettingsHelperButtons(
        canSwitch: true,
        canCheck: true,
        canRecover: true,
        canLogOut: true,
        switchHelp: "Lock your screen so you can log in to the helper"
      )
    )

    /// Mouse and keyboard stopped in the helper: its row says so in short and offers Recover… beside it, the one
    /// thing that fixes it.
    @MainActor static let settingsHelperNotReady: SettingsState = {
      var state = selecting(.helper)
      state.helper.account.status = StatusSummary(label: "Needs you", tone: .attention, mark: .dot)
      state.helper.ready[1] = SettingsReadyRow(
        item: ReadinessItem(name: "Mouse and keyboard", text: "Stopped in the helper.", state: .notReady),
        offersRecover: true,
        index: 1
      )
      state.helper.ready[2].item = ReadinessItem(
        name: "Save and Open dialogs",
        text: "Not checked yet",
        state: .unknown
      )
      return state
    }()

    /// Another window's Log Out runs: every button waits under a line that names it, and Check Again found the
    /// helper isn't ready, under "Ready to act".
    @MainActor static let settingsHelperWaiting: SettingsState = {
      var state = settingsHelperNotReady
      state.helper.buttons.canSwitch = false
      state.helper.buttons.canRecover = false
      state.helper.buttons.canLogOut = false
      state.helper.waiting = "Log Out is running. Try again once it ends."
      state.helper.readyNote = SettingsNote(text: "The helper isn't ready to act.", kind: .problem)
      return state
    }()

    /// A deleted account's login is still open: Close It… comes first.
    @MainActor static let settingsHelperLeftover: SettingsState = {
      var state = selecting(.helper)
      state.helper.closeLogin = SettingsCloseLoginRow(
        title: "A deleted account is still logged in",
        detail: "Closes it, with your approval, so the next login doesn't stall",
        button: "Close It…",
        event: .closeLeftovers,
        canRun: true
      )
      return state
    }()

    /// Atlas closes it, under the person's approval.
    @MainActor static let settingsHelperClosing: SettingsState = {
      var state = settingsHelperLeftover
      state.helper.closeLogin?.canRun = false
      state.helper.closeLogin?.status = "Closing…"
      return state
    }()

    /// The helper is logged in while Atlas there is silent: Log Out… as root.
    @MainActor static let settingsHelperStale: SettingsState = {
      var state = selecting(.helper)
      state.helper.account.status = StatusSummary(label: "Needs you", tone: .attention, mark: .dot)
      state.helper.closeLogin = SettingsCloseLoginRow(
        title: "The helper is still logged in",
        detail: "Atlas isn't answering there, so log it out to start it again",
        button: "Log Out…",
        event: .closeHelper,
        canRun: true
      )
      return state
    }()
  }
#endif
