#if DEBUG
  extension PreviewStates {
    /// The panel sample an event leads to, so the Interactive preview answers clicks as the app would.
    ///
    /// - Parameters:
    ///   - state: The sample shown.
    ///   - event: What was clicked.
    /// - Returns: The sample to show next.
    @MainActor package static func panel(_ state: PanelState, applying event: PanelEvent) -> PanelState {
      var next = state
      switch event {
      case .askToLogOut:
        next.feedback = nil
        next.logOut = .confirming(isEnabled: true)
        next.menu.logOut = .disabled
      case .cancelLogOut:
        next.logOut = .none
        next.menu.logOut = .enabled
      case .logOut, .forceLogOut:
        return panelLoggedOut
      case .revokeControl, .logIn, .recover, .resumeAgent:
        return panelWatching
      case .stopAgent:
        return panelStopped
      case .setOpenAtLogin(let on):
        next.menu.opensAtLogin = on
      case .openViewer, .switchToHelper, .gettingStarted, .quit:
        break
      }
      return next
    }
  }
#endif
