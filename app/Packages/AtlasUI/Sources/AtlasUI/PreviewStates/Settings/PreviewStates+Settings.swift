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
      ]
    }

    /// The four tabs, each its symbol over its word (v3 G1).
    static let settingsTabs = [
      CapsuleTab(id: SettingsTab.general, title: "General", symbol: .gear),
      CapsuleTab(id: SettingsTab.helper, title: "Helper", symbol: .display),
      CapsuleTab(id: SettingsTab.agents, title: "Agents", symbol: .sparkles),
      CapsuleTab(id: SettingsTab.about, title: "About", symbol: .info),
    ]

    /// v3's General pane: opening at login and notifying on, the picture live, no badge; the other panes as v3 draws
    /// them.
    @MainActor static let settingsGeneral = SettingsState(
      tabs: CapsuleTabsState(tabs: settingsTabs, selected: .general, label: "Settings"),
      general: SettingsGeneralState(
        opensAtLogin: true,
        canOpenAtLogin: true,
        picture: .live,
        pictureOptions: [
          PopupOption(id: .live, title: "Live"),
          PopupOption(id: .everyFiveSeconds, title: "Every 5 seconds"),
          PopupOption(id: .paused, title: "Paused", isEnabled: false),
        ],
        notify: true,
        badge: false
      ),
      helper: settingsHelperPane,
      agents: settingsAgentsPane,
      about: SettingsAboutState(version: "Version 0.1.0", update: .check)
    )

    /// v3's Helper pane.
    @MainActor static let settingsHelper: SettingsState = selecting(.helper)

    /// v3's Agents pane.
    @MainActor static let settingsAgents: SettingsState = selecting(.agents)

    /// v3's About pane.
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
