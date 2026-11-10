#if DEBUG
  extension PreviewStates {
    /// The person signs the helper in to github.com, agents paused and the screen hidden from them: the bar over the
    /// Screen, the tag in the header and the window in control.
    @MainActor public static let viewerSigningIn: ViewerState = {
      var state = viewerControl
      state.header.tag = "Screen · Hidden from agents"
      state.screen.signingIn = SigningInBarState(
        text: "Signing in to github.com. Agents are paused and can't see the screen."
      )
      return state
    }()

    /// Done: github.com joined the accounts.
    @MainActor public static let viewerSignedIn: ViewerState = {
      var state = viewerWorking
      state.sidebar.helper.menu = viewerHelperMenu(alertUp: true)
      state.alert = AlertCardState(
        title: "Added github.com to Accounts",
        message: "Agents ask you before sending there.",
        buttons: [AlertCardButton(id: .confirm, title: "OK", role: .action)]
      )
      return state
    }()

    /// Done after Forget This Site…: whether to take github.com off the list too, Keep the default.
    @MainActor public static let viewerForgotten: ViewerState = {
      var state = viewerWorking
      state.sidebar.helper.menu = viewerHelperMenu(alertUp: true)
      state.alert = AlertCardState(
        title: "Remove github.com from Accounts too?",
        message: "Agents ask you before sending there while it stays listed.",
        buttons: [
          AlertCardButton(id: .cancel, title: "Keep", role: .safeCancel),
          AlertCardButton(id: .confirm, title: "Remove", role: .destructive),
        ]
      )
      return state
    }()
  }
#endif
