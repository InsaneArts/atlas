#if DEBUG
  extension PreviewStates {
    /// The Settings sample an event leads to, so the Interactive preview answers clicks as the app would.
    ///
    /// - Parameters:
    ///   - state: The sample shown.
    ///   - event: What was clicked.
    /// - Returns: The sample to show next.
    @MainActor public static func settings(_ state: SettingsState, applying event: SettingsEvent) -> SettingsState {
      var next = state
      switch event {
      case .selectTab(let tab):
        next.tabs.selected = tab
      case .askRecover:
        next.alert = settingsRecover.alert
      case .askLogOut:
        next.alert = settingsLogOut.alert
      case .askForget:
        next.alert = settingsForget.alert
      case .askInstall:
        next.alert = settingsInstall.alert
      case .checkForUpdates:
        next.about = settingsAboutNewer.about
      case .connect(let id):
        if let index = next.agents.agents.firstIndex(where: { $0.id == id }) {
          next.agents.agents[index].connection = .connected
        }
      case .alertConfirm:
        if next.alert == settingsForget.alert { next.agents.projects.removeFirst() }
        if next.alert == settingsInstall.alert { next.about = settingsAbout.about }
        next.alert = nil
      case .alertCancel:
        next.alert = nil
      case .askSwitch:
        next.alert = settingsSwitch.alert
      case .closeLeftovers, .closeHelper:
        next.helper.closeLogin = nil
      case .checkAgain, .gettingStarted, .paneHeight, .showAppInFinder, .showAgentFile:
        break
      }
      return next
    }
  }
#endif
