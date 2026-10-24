#if DEBUG
  extension PreviewStates {
    /// Every alert window sample, in the order the previews and the renders list them.
    @MainActor public static var alert: [PreviewState<AlertCardState<AlertChoice>>] {
      [
        PreviewState(name: "Recover", value: alertRecover),
        PreviewState(name: "Switch", value: alertSwitch),
        PreviewState(name: "Log out", value: alertLogOut),
        PreviewState(name: "Log out anyway", value: alertLogOutAnyway),
        PreviewState(name: "Recover refused", value: alertRecoverRefused),
        PreviewState(name: "Recovering", value: alertRecovering),
        PreviewState(name: "Recovered", value: alertRecovered),
        PreviewState(name: "Logging out", value: alertLoggingOut),
      ]
    }

    /// Recover the helper, asked from the panel while an action may not have finished.
    static let alertRecover = AlertCardState<AlertChoice>(
      title: "Recover the helper?",
      message: "An action may not have finished. Atlas checks the helper and starts its mouse and keyboard again.",
      buttons: [
        AlertCardButton(id: .confirm, title: "Recover", role: .action),
        AlertCardButton(id: .cancel, title: "Not Now", role: .cancel),
      ]
    )

    /// Switch to the helper.
    static let alertSwitch = AlertCardState<AlertChoice>(
      title: "Switch to the helper?",
      message:
        "Your screen locks so you can log in to the helper. Everything here keeps running, and you can switch back any time.",
      buttons: [
        AlertCardButton(id: .confirm, title: "Switch", role: .action),
        AlertCardButton(id: .cancel, title: "Cancel", role: .cancel),
      ]
    )

    /// Log out the helper: a destructive alert, Cancel first.
    static let alertLogOut = AlertCardState<AlertChoice>(
      title: "Log out the helper?",
      message: "Its apps quit, and agents can't use it until you log in to it again.",
      buttons: [
        AlertCardButton(id: .cancel, title: "Cancel", role: .safeCancel),
        AlertCardButton(id: .confirm, title: "Log Out", role: .destructive),
      ]
    )

    /// An app held the log-out up: Log Out Anyway.
    static let alertLogOutAnyway = AlertCardState<AlertChoice>(
      title: "The helper didn't log out",
      message: "An app in the helper is holding it up. Log Out Anyway quits it, and anything unsaved there is lost.",
      buttons: [
        AlertCardButton(id: .cancel, title: "Cancel", role: .safeCancel),
        AlertCardButton(id: .confirm, title: "Log Out Anyway", role: .destructive),
      ]
    )

    /// Recover while it runs: a spinner in Recover, which says what it does, while Not Now waits.
    static let alertRecovering: AlertCardState<AlertChoice> = {
      var state = alertRecover
      state.phase = .running(label: "Recovering…")
      return state
    }()

    /// Log Out while it runs: a red spinner in Log Out, which says what it does, while Cancel waits.
    static let alertLoggingOut: AlertCardState<AlertChoice> = {
      var state = alertLogOut
      state.phase = .running(label: "Logging Out…")
      return state
    }()

    /// Recover once it worked: a check in the button and Recovered as the title, just before it closes.
    static let alertRecovered: AlertCardState<AlertChoice> = {
      var state = alertRecover
      state.phase = .done(title: "Recovered")
      return state
    }()

    /// A Recover the helper refused while it was in use: why, as a problem line in place of the sentence, with
    /// the buttons back to try again.
    static let alertRecoverRefused: AlertCardState<AlertChoice> = {
      var state = alertRecover
      state.phase = .refused(message: "The helper is in use. Try Recover again once it's free.")
      return state
    }()
  }
#endif
