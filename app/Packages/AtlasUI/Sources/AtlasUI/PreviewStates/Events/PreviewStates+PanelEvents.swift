#if DEBUG
  extension PreviewStates {
    /// The panel sample an event leads to, so the Interactive preview answers clicks as the app would: Stop stops,
    /// Resume and Give Back go back to idle, the pause button pauses or resumes the picture, OK takes the service
    /// note away, an answer takes the question away and End what the person allowed. What opens a window,
    /// an alert or a menu changes nothing here.
    ///
    /// - Parameters:
    ///   - state: The sample shown.
    ///   - event: What was clicked.
    /// - Returns: The sample to show next.
    @MainActor public static func panel(_ state: PanelState, applying event: PanelEvent) -> PanelState {
      switch event {
      case .stopAgent:
        return panelStopped
      case .resumeAgent, .giveBack:
        return panelIdle
      case .togglePause:
        return state.picture.pause.isPaused ? panelIdle : panelPaused
      case .toggleSound:
        var next = state
        next.picture.sound = HelperSoundButton(isMuted: !state.picture.sound.isMuted)
        return next
      case .dismissServiceNote:
        var next = state
        next.showsServiceNote = false
        return next
      case .dismissIdleNote:
        var next = state
        next.idleNote = nil
        return next
      case .answerAsk:
        var next = state
        next.ask = nil
        return next
      case .endAllowed(let id):
        var next = state
        next.allowed.removeAll { $0.id == id }
        return next
      case .openAtlas, .openScreen, .openActivity, .openSession, .showAllSessions, .askRecover, .askSwitch,
        .openSettings,
        .openAccounts,
        .showMore:
        return state
      }
    }
  }
#endif
