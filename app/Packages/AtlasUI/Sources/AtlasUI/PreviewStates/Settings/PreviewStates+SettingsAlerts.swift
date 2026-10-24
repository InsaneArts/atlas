#if DEBUG
  extension PreviewStates {
    /// Recover asked for from the Helper pane, with the main window's words.
    @MainActor static let settingsRecover: SettingsState = {
      var state = selecting(.helper)
      state.alert = AlertCardState(
        title: "Recover the helper?",
        message: "Atlas checks the helper and starts it again.",
        buttons: [
          AlertCardButton(id: .confirm, title: "Recover", role: .action),
          AlertCardButton(id: .cancel, title: "Cancel", role: .cancel),
        ]
      )
      return state
    }()

    /// Log Out asked for from the Helper pane, with the main window's words.
    @MainActor static let settingsLogOut: SettingsState = {
      var state = selecting(.helper)
      state.alert = AlertCardState(
        title: "Log out the helper?",
        message: "Its apps quit, and agents can't use it until you log in to it again.",
        buttons: [
          AlertCardButton(id: .cancel, title: "Cancel", role: .safeCancel),
          AlertCardButton(id: .confirm, title: "Log Out", role: .destructive),
        ]
      )
      return state
    }()

    /// Forget asked for a project: it stays listed until the person confirms.
    @MainActor static let settingsForget: SettingsState = {
      var state = selecting(.agents)
      state.alert = AlertCardState(
        title: "Forget “opie”?",
        message: "Atlas removes its instructions from this project. Agents there stop using the helper.",
        buttons: [
          AlertCardButton(id: .cancel, title: "Cancel", role: .safeCancel),
          AlertCardButton(id: .confirm, title: "Forget", role: .destructive),
        ]
      )
      return state
    }()

    /// Install asked for the newer version found.
    @MainActor static let settingsInstall: SettingsState = {
      var state = settingsAboutNewer
      state.alert = AlertCardState(
        title: "Install Atlas 0.2.0?",
        message: "Atlas checks the release's signature and installs it, then restarts itself to finish.",
        buttons: [
          AlertCardButton(id: .confirm, title: "Install", role: .action),
          AlertCardButton(id: .cancel, title: "Cancel", role: .cancel),
        ]
      )
      return state
    }()

    /// Switch asked for from the Helper pane, with the main window's words.
    @MainActor static let settingsSwitch: SettingsState = {
      var state = selecting(.helper)
      state.alert = AlertCardState(
        title: "Switch to the helper?",
        message:
          "Your screen locks so you can log in to the helper. Everything here keeps running, and you can switch back any time.",
        buttons: [
          AlertCardButton(id: .confirm, title: "Switch", role: .action),
          AlertCardButton(id: .cancel, title: "Cancel", role: .cancel),
        ]
      )
      return state
    }()

    /// An app in the helper held the log-out up: Log Out Anyway asks again.
    @MainActor static let settingsLogOutAnyway: SettingsState = {
      var state = selecting(.helper)
      state.alert = AlertCardState(
        title: "The helper didn't log out",
        message:
          "An app in the helper is holding it up. Log Out Anyway quits it, and anything unsaved there is lost.",
        buttons: [
          AlertCardButton(id: .cancel, title: "Cancel", role: .safeCancel),
          AlertCardButton(id: .confirm, title: "Log Out Anyway", role: .destructive),
        ]
      )
      return state
    }()

    /// Recover running from its alert: a spinner in Recover while Cancel waits.
    @MainActor static let settingsRecovering: SettingsState = {
      var state = settingsRecover
      state.alert?.phase = .running(label: "Recovering…")
      state.helper.buttons.canRecover = false
      return state
    }()
  }
#endif
