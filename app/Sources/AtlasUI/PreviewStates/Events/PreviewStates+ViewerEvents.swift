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
      case .recoverLater, .showRecovery, .showKeys, .setRate:
        return state
      }
    }
  }
#endif
