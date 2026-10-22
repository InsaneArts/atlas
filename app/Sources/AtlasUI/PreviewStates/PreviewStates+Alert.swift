#if DEBUG
  extension PreviewStates {
    /// Every alert window sample, in the order the previews and the renders list them.
    @MainActor package static var alert: [PreviewState<OrbitAlertState<AlertChoice>>] {
      [
        PreviewState(name: "Recover", value: alertRecover),
        PreviewState(name: "Switch", value: alertSwitch),
        PreviewState(name: "Log out", value: alertLogOut),
        PreviewState(name: "Log out anyway", value: alertLogOutAnyway),
        PreviewState(name: "Recover refused", value: alertRecoverRefused),
        PreviewState(name: "Recovering", value: alertRecovering),
      ]
    }

    /// Recover the helper, asked from the panel while an action may not have finished.
    static let alertRecover = OrbitAlertState<AlertChoice>(
      title: "Recover the helper?",
      message: "An action may not have finished. Atlas checks the helper and starts its mouse and keyboard again.",
      buttons: [
        OrbitAlertButton(id: .confirm, title: "Recover", role: .action),
        OrbitAlertButton(id: .cancel, title: "Not Now", role: .cancel),
      ]
    )

    /// Switch to the helper.
    static let alertSwitch = OrbitAlertState<AlertChoice>(
      title: "Switch to the helper?",
      message:
        "Your screen locks so you can log in to the helper. Everything here keeps running, and you can switch back any time.",
      buttons: [
        OrbitAlertButton(id: .confirm, title: "Switch", role: .action),
        OrbitAlertButton(id: .cancel, title: "Cancel", role: .cancel),
      ]
    )

    /// Log out the helper: a destructive alert, Cancel first.
    static let alertLogOut = OrbitAlertState<AlertChoice>(
      title: "Log out the helper?",
      message: "Its apps quit, and agents can't use it until you log in to it again.",
      buttons: [
        OrbitAlertButton(id: .cancel, title: "Cancel", role: .safeCancel),
        OrbitAlertButton(id: .confirm, title: "Log Out", role: .destructive),
      ]
    )

    /// An app held the log-out up: Log Out Anyway.
    static let alertLogOutAnyway = OrbitAlertState<AlertChoice>(
      title: "The helper didn't log out",
      message: "An app in the helper is holding it up. Log Out Anyway quits it, and anything unsaved there is lost.",
      buttons: [
        OrbitAlertButton(id: .cancel, title: "Cancel", role: .safeCancel),
        OrbitAlertButton(id: .confirm, title: "Log Out Anyway", role: .destructive),
      ]
    )

    /// Recover while it runs: its buttons wait, and its action says what it does.
    static let alertRecovering: OrbitAlertState<AlertChoice> = {
      var state = alertRecover
      state.buttons[0].title = "Recovering…"
      state.isBusy = true
      return state
    }()

    /// A Recover the helper refused while it was in use: why, in place of the sentence, with Recover to try again.
    static let alertRecoverRefused: OrbitAlertState<AlertChoice> = {
      var state = alertRecover
      state.message = "The helper is in use. Try Recover again once it's free."
      return state
    }()
  }
#endif
