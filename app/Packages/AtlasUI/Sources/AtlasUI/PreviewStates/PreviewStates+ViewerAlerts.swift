#if DEBUG
  import CoreGraphics

  extension PreviewStates {
    /// An action may not have finished: the picture dims under the recovery alert.
    @MainActor public static let viewerNeedsYou = ViewerState(
      top: viewerTop(
        StatusSummary(label: "Needs you", tone: .attention, mark: .dot, line: "An action may not have finished"),
        viewerActions(
          keys: .hidden,
          control: ViewerControlButton(
            title: "Recover…",
            isEnabled: true,
            help: "Check the helper and start its mouse and keyboard again",
            event: .askRecover,
            titles: viewerControlTitles
          )
        )
      ),
      screen: ViewerScreenState(picture: SampleDesktop.picture, look: .dimmed),
      alert: AlertCardState(
        title: "Recover the helper?",
        message:
          "An action may not have finished. Atlas checks the helper and starts its mouse and keyboard again.",
        buttons: [
          AlertCardButton(id: .confirm, title: "Recover", role: .action),
          AlertCardButton(id: .cancel, title: "Not Now", role: .cancel),
        ]
      )
    )

    /// Switch to the Helper… asks first, over a logged-out helper.
    @MainActor public static let viewerSwitch: ViewerState = {
      var state = viewerLoggedOut
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

    /// Log Out the Helper… asks first: Cancel in blue, Log Out in red.
    @MainActor public static let viewerLogOut: ViewerState = {
      var state = viewerIdle
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

    /// An app held the log-out up: Log Out Anyway is offered.
    @MainActor public static let viewerLogOutAnyway: ViewerState = {
      var state = viewerIdle
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

    /// Restart Helper… asks first: anything an agent is doing stops, so Cancel is the default and Restart red.
    @MainActor public static let viewerRestart: ViewerState = {
      var state = viewerIdle
      state.alert = AlertCardState(
        title: "Restart the helper?",
        message: "Atlas starts its service in the helper again. Anything an agent is doing there stops.",
        buttons: [
          AlertCardButton(id: .cancel, title: "Cancel", role: .safeCancel),
          AlertCardButton(id: .confirm, title: "Restart", role: .destructive),
        ]
      )
      return state
    }()

    /// The helper needs the person, at actual size: the picture dims under the alert as the fitted one does.
    @MainActor public static let viewerActualSizeNeedsYou: ViewerState = {
      var state = viewerNeedsYou
      state.screen.actualSize = CGSize(width: 1440, height: 900)
      return state
    }()
  }
#endif
