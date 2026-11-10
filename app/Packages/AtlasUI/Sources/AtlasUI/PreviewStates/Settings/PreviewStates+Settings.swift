#if DEBUG
  import SwiftUI

  extension PreviewStates {
    /// Every Settings window sample, in the order the previews list them.
    @MainActor public static var settings: [PreviewState<SettingsState>] {
      [
        PreviewState(name: "Helper", value: settingsHelper),
        PreviewState(name: "General", value: settingsGeneral),
        PreviewState(name: "Activity", value: settingsActivity),
        PreviewState(name: "Accounts", value: settingsAccounts),
        PreviewState(name: "Privacy", value: settingsPrivacy),
        PreviewState(name: "Integrations", value: settingsAgents),
        PreviewState(name: "About", value: settingsAbout),
        PreviewState(name: "Activity, custom days and a limit", value: settingsGeneralCustom),
        PreviewState(name: "Helper not ready", value: settingsHelperNotReady),
        PreviewState(name: "Integrations connecting", value: settingsAgentsConnecting),
        PreviewState(name: "Integrations with no projects", value: settingsAgentsEmpty),
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
        PreviewState(name: "Privacy, home folder open", value: settingsHelperHomeOpen),
        PreviewState(name: "Privacy, home folder failed", value: settingsHelperHomeFailed),
        PreviewState(name: "Privacy, sign-in copies allowed", value: settingsHelperCopiesAllowed),
        PreviewState(name: "Privacy, files unread", value: settingsPrivacyUnread),
        PreviewState(name: "Let the helper open the home folder", value: settingsOpenHome),
        PreviewState(name: "Let agents copy sign-ins", value: settingsAllowSignIns),
        PreviewState(name: "Accounts, none", value: settingsAccountsEmpty),
        PreviewState(name: "Accounts, one added", value: settingsAccountsAdded),
        PreviewState(name: "Accounts, unread", value: settingsAccountsUnread),
        PreviewState(name: "Remove an account", value: settingsRemoveAccount),
        PreviewState(name: "Accounts, look only", value: settingsAccountsLookOnly),
        PreviewState(name: "Sign in to a site", value: settingsSignIn),
        PreviewState(name: "Sign in to a site, refused", value: settingsSignInRefused),
        PreviewState(name: "Account, a site", value: settingsAccountOpened),
        PreviewState(name: "Account, an app", value: settingsAccountApp),
        PreviewState(name: "Account, list unreadable", value: settingsAccountUnreadable),
        PreviewState(name: "Account, a change failed, list unreadable", value: settingsAccountFailedUnreadable),
        PreviewState(name: "Account, a long name", value: settingsAccountLongName),
        PreviewState(name: "Accounts, pictures off", value: settingsAccountsPicturesOff),
      ]
    }

    /// The sidebar: every page, Helper saying the helper is ready.
    static let settingsSidebar = SettingsSidebarState(
      rows: [
        SettingsSidebarRow(page: .helper, title: "Helper", subtitle: "Ready", tone: .ready),
        SettingsSidebarRow(page: .general, title: "General"),
        SettingsSidebarRow(page: .activity, title: "Activity"),
        SettingsSidebarRow(page: .accounts, title: "Accounts"),
        SettingsSidebarRow(page: .privacy, title: "Privacy"),
        SettingsSidebarRow(page: .integrations, title: "Integrations"),
        SettingsSidebarRow(page: .about, title: "About"),
      ]
    )

    /// The General page: opening at login, muting the helper and notifying on, the screen live; the other pages at
    /// rest.
    @MainActor static let settingsGeneral = SettingsState(
      route: .page(.general),
      sidebar: settingsSidebar,
      general: SettingsGeneralState(
        opensAtLogin: true,
        canOpenAtLogin: true,
        picture: .live,
        pictureOptions: [
          PopupOption(id: .live, title: "Live"),
          PopupOption(id: .everyFiveSeconds, title: "Every 5 s"),
          PopupOption(id: .paused, title: "Paused", isEnabled: false),
        ],
        muteHelper: true,
        soundProblem: nil,
        storedPictures: "1,240 pictures · 142 MB",
        canDeletePictures: true,
        notify: true
      ),
      helper: settingsHelperPane,
      accounts: settingsAccountsPane,
      privacy: settingsFilesPrivate,
      agents: settingsAgentsPane,
      about: SettingsAboutState(version: "Version 0.1.0", update: .check)
    )

    /// The Activity page with pictures kept a custom 14 days and typed text limited to 80 characters.
    @MainActor static let settingsGeneralCustom: SettingsState = {
      var state = selecting(.activity)
      state.general.keepFor = .custom
      state.general.keepDays = 14
      state.general.typedText = .limited
      return state
    }()

    /// The Helper page.
    @MainActor static let settingsHelper: SettingsState = selecting(.helper)

    /// The Activity page.
    @MainActor static let settingsActivity: SettingsState = selecting(.activity)

    /// The Privacy page.
    @MainActor static let settingsPrivacy: SettingsState = selecting(.privacy)

    /// The Integrations page.
    @MainActor static let settingsAgents: SettingsState = selecting(.integrations)

    /// The About page.
    @MainActor static let settingsAbout: SettingsState = selecting(.about)

    /// The General sample with another page shown, come to from General.
    ///
    /// - Parameter page: The page to show.
    /// - Returns: The sample.
    @MainActor static func selecting(_ page: SettingsPage) -> SettingsState {
      var state = settingsGeneral
      state.route = .page(page)
      return state
    }
  }
#endif
