#if DEBUG
  extension PreviewStates {
    /// The panel sample an event leads to, so the Interactive preview answers clicks as the app would: Stop stops,
    /// Resume and Give Back go back to idle, and the pause button pauses or resumes the picture. What opens a window,
    /// an alert or a menu changes nothing here.
    ///
    /// - Parameters:
    ///   - state: The sample shown.
    ///   - event: What was clicked.
    /// - Returns: The sample to show next.
    @MainActor package static func panel(_ state: PanelState, applying event: PanelEvent) -> PanelState {
      switch event {
      case .stopAgent:
        return panelStopped
      case .resumeAgent, .giveBack:
        return panelIdle
      case .togglePause:
        return state.controls.pause.isPaused ? panelIdle : panelPaused
      case .openAtlas, .openScreen, .openActivity, .askRecover, .askSwitch, .showMore:
        return state
      }
    }
  }
#endif
