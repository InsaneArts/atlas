#if DEBUG
  extension PreviewStates {
    /// The viewer sample an event leads to, so the Interactive preview answers clicks as the app would.
    ///
    /// - Parameters:
    ///   - state: The sample shown.
    ///   - event: What was clicked.
    /// - Returns: The sample to show next.
    @MainActor package static func viewer(_ state: ViewerState, applying event: ViewerEvent) -> ViewerState {
      switch event {
      case .toggleSidebar:
        var next = state
        next.header.sidebar = viewerSidebarToggle(shown: !state.header.sidebar.isOn)
        return next
      case .togglePause:
        return state.footer.pause.isOn ? viewerWatching : viewerPaused
      case .takeControl:
        return viewerControl
      case .giveBack, .recover, .logIn:
        return viewerWatching
      case .stopAgent:
        var next = state
        next.footer.agent = .resume(isEnabled: true)
        return next
      case .resumeAgent:
        var next = state
        next.footer.agent = .stop
        return next
      case .recoverLater, .showRecovery, .showKeys, .setRate:
        return state
      }
    }
  }
#endif
