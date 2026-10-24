#if DEBUG
  extension PreviewStates {
    /// v3's Helper pane: the helper idle at 2560 × 1440, every check ready, every button free.
    static let settingsHelperPane = SettingsHelperState(
      account: SettingsHelperAccount(
        name: "Atlas",
        line: "The hidden helper login · 2560 × 1440",
        status: StatusSummary(label: "Idle", tone: .quiet, mark: .dot)
      ),
      ready: [
        SettingsReadyRow(item: ReadinessItem(name: "Helper is running", text: "", state: .ready)),
        SettingsReadyRow(item: ReadinessItem(name: "Mouse and keyboard", text: "", state: .ready)),
        SettingsReadyRow(
          item: ReadinessItem(name: "Save and Open dialogs", text: "", state: .ready),
          offersCheck: true
        ),
      ],
      buttons: SettingsHelperButtons(
        canSwitch: true,
        canCheck: true,
        canRecover: true,
        canLogOut: true,
        switchHelp: "Lock your screen so you can log in to the helper"
      )
    )

    /// Mouse and keyboard stopped in the helper: its check says what to do in its own row, and while another
    /// action runs, Recover and Log Out wait under a line saying why (v3 G5, G6).
    @MainActor static let settingsHelperNotReady: SettingsState = {
      var state = selecting(.helper)
      state.helper.account.status = StatusSummary(label: "Needs you", tone: .attention, mark: .dot)
      state.helper.ready[1].item = ReadinessItem(
        name: "Mouse and keyboard",
        text: "Recover starts the helper's mouse and keyboard again.",
        state: .notReady
      )
      state.helper.ready[2].item = ReadinessItem(
        name: "Save and Open dialogs",
        text: "Not checked yet.",
        state: .unknown
      )
      state.helper.buttons.canRecover = false
      state.helper.buttons.canLogOut = false
      state.helper.waiting = "Another action is running. Try again once it ends."
      return state
    }()
  }
#endif
