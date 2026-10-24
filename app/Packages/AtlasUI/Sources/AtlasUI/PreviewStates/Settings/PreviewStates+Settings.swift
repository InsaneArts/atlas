#if DEBUG
  import SwiftUI

  extension PreviewStates {
    /// Every Settings window sample, in the order the previews list them.
    @MainActor public static var settings: [PreviewState<SettingsState>] {
      [
        PreviewState(name: "General", value: settingsGeneral),
        PreviewState(name: "Helper", value: settingsHelper),
        PreviewState(name: "Agents", value: settingsAgents),
        PreviewState(name: "About", value: settingsAbout),
        PreviewState(name: "Helper not ready", value: settingsHelperNotReady),
        PreviewState(name: "Agents connecting", value: settingsAgentsConnecting),
        PreviewState(name: "Agents with no projects", value: settingsAgentsEmpty),
        PreviewState(name: "Checking for updates", value: settingsAboutChecking),
        PreviewState(name: "Up to date", value: settingsAboutUpToDate),
        PreviewState(name: "Update available", value: settingsAboutNewer),
        PreviewState(name: "Update check failed", value: settingsAboutFailed),
        PreviewState(name: "Recover", value: settingsRecover),
        PreviewState(name: "Log out", value: settingsLogOut),
        PreviewState(name: "Forget", value: settingsForget),
        PreviewState(name: "Install", value: settingsInstall),
        PreviewState(name: "Switch", value: settingsSwitch),
        PreviewState(name: "Log out anyway", value: settingsLogOutAnyway),
        PreviewState(name: "Recovering", value: settingsRecovering),
        PreviewState(name: "Installing", value: settingsAboutInstalling),
        PreviewState(name: "Install failed", value: settingsAboutInstallFailed),
        PreviewState(name: "No public release", value: settingsAboutUnpublished),
        PreviewState(name: "Helper waiting", value: settingsHelperWaiting),
        PreviewState(name: "Helper, a deleted account still logged in", value: settingsHelperLeftover),
        PreviewState(name: "Helper, closing it", value: settingsHelperClosing),
        PreviewState(name: "Helper, still logged in", value: settingsHelperStale),
        PreviewState(name: "Claude Code only", value: settingsClaudeCode),
        PreviewState(name: "Claude app only", value: settingsClaudeApp),
        PreviewState(name: "Claude partly connected", value: settingsClaudePartial),
      ]
    }

    /// The four tabs, each its symbol over its word.
    static let settingsTabs = [
      CapsuleTab(id: SettingsTab.general, title: "General", symbol: .gear),
      CapsuleTab(id: SettingsTab.helper, title: "Helper", symbol: .display),
      CapsuleTab(id: SettingsTab.agents, title: "Agents", symbol: .sparkles),
      CapsuleTab(id: SettingsTab.about, title: "About", symbol: .info),
    ]

    /// The General pane: opening at login and notifying on, the screen live; the other panes at rest.
    @MainActor static let settingsGeneral = SettingsState(
      tabs: CapsuleTabsState(tabs: settingsTabs, selected: .general, label: "Settings"),
      general: SettingsGeneralState(
        opensAtLogin: true,
        canOpenAtLogin: true,
        picture: .live,
        pictureOptions: [
          PopupOption(id: .live, title: "Live"),
          PopupOption(id: .everyFiveSeconds, title: "Every 5 Seconds"),
          PopupOption(id: .paused, title: "Paused", isEnabled: false),
        ],
        notify: true
      ),
      helper: settingsHelperPane,
      agents: settingsAgentsPane,
      about: SettingsAboutState(version: "Version 0.1.0", update: .check)
    )

    /// The Helper pane.
    @MainActor static let settingsHelper: SettingsState = selecting(.helper)

    /// The Agents pane.
    @MainActor static let settingsAgents: SettingsState = selecting(.agents)

    /// The About pane.
    @MainActor static let settingsAbout: SettingsState = selecting(.about)

    /// The General sample with another pane selected.
    ///
    /// - Parameter tab: The pane to show.
    /// - Returns: The sample.
    @MainActor static func selecting(_ tab: SettingsTab) -> SettingsState {
      var state = settingsGeneral
      state.tabs.selected = tab
      return state
    }
  }
#endif
