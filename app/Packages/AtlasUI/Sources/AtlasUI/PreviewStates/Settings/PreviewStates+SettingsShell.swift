#if DEBUG
  extension PreviewStates {
    /// A long account name and connecting status for checking the unchanged pane inside the new shell.
    @MainActor public static var settingsLongHelper: SettingsState {
      var state = settingsHelper
      state.helper.account.name = "Atlas's helper account with a very long display name"
      state.helper.account.status = StatusSummary(label: "Connecting…", tone: .quiet, mark: .small)
      state.canGoBack = true
      state.canGoForward = true
      return state
    }
  }
#endif
