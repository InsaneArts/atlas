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

    /// The four tabs, the board's names and icons.
    static let settingsTabs = [
      IconTab(id: SettingsTab.general, title: "General", icon: .symbol("gearshape")),
      IconTab(id: SettingsTab.helper, title: "Helper", icon: .symbol("display")),
      IconTab(id: SettingsTab.agents, title: "Agents", icon: .symbol("person.2")),
      IconTab(id: SettingsTab.about, title: "About", icon: .orbit),
    ]

    /// The board's General pane: opening at login and notifying on, the picture live, no badge.
    @MainActor static let settingsGeneral = SettingsState(
      tabs: IconTabsState(tabs: settingsTabs, selected: .general, label: "Settings"),
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
      helper: SettingsHelperState(
        account: "Atlas",
        status: StatusSummary(label: "Idle", tone: .quiet, mark: .dot),
        line: "The hidden helper login · 2560 × 1440",
        ready: [
          SettingsReadyRow(item: ReadinessItem(name: "Helper is running", text: "", state: .ready)),
          SettingsReadyRow(item: ReadinessItem(name: "Mouse and keyboard", text: "", state: .ready)),
          SettingsReadyRow(
            item: ReadinessItem(name: "Save and Open dialogs", text: "", state: .ready),
            offersCheck: true
          ),
        ],
        buttons: SettingsHelperButtons(
          canSwitch: true,
          canCheck: true,
          canRecover: true,
          canLogOut: true,
          switchHelp: "Lock your screen so you can log in to the helper"
        )
      ),
      agents: SettingsAgentsState(
        agents: [
          SettingsAgentRow(
            id: "claude",
            title: "Claude Code",
            detail: "Uses Atlas in every project",
            icon: .tile(.symbol("asterisk")),
            connection: .connected
          ),
          SettingsAgentRow(
            id: "codex",
            title: "Codex",
            detail: "Uses Atlas in every project",
            icon: .tile(.text(">_")),
            connection: .connected
          ),
          SettingsAgentRow(
            id: "claude-app",
            title: "Claude app",
            detail: "Shows the helper in your chats",
            icon: .tile(.symbol("bubble.left")),
            connection: .connect(isEnabled: true)
          ),
        ],
        projects: [
          SettingsProjectRow(id: "/Users/me/dev/opie", name: "opie", detail: "~/dev/opie", forgetLabel: "Forget opie"),
          SettingsProjectRow(
            id: "/Users/me/dev/atlas-landing",
            name: "atlas-landing",
            detail: "No longer has the Atlas section",
            forgetLabel: "Forget atlas-landing"
          ),
        ]
      ),
      about: SettingsAboutState(version: "Version 0.1.0", update: .check)
    )

    /// The board's Helper pane.
    @MainActor static let settingsHelper: SettingsState = selecting(.helper)

    /// The board's Agents pane.
    @MainActor static let settingsAgents: SettingsState = selecting(.agents)

    /// The board's About pane.
    @MainActor static let settingsAbout: SettingsState = selecting(.about)

    /// Mouse and keyboard stopped in the helper: its check says what to do, and Recover is where to do it.
    @MainActor static let settingsHelperNotReady: SettingsState = {
      var state = selecting(.helper)
      state.helper.status = StatusSummary(label: "Not ready", tone: .quiet, mark: .small)
      state.helper.ready[1].item = ReadinessItem(
        name: "Mouse and keyboard",
        text: "Recover starts the helper's mouse and keyboard again.",
        state: .notReady
      )
      state.helper.ready[2].item = ReadinessItem(
        name: "Save and Open dialogs",
        text: "Not checked yet.",
        state: .unknown
      )
      state.helper.buttons.canLogOut = false
      state.helper.notice = SettingsNote(text: "Another action is running. Try again once it ends.", isProblem: true)
      return state
    }()

    /// The Claude app connecting, the others waiting for it.
    @MainActor static let settingsAgentsConnecting: SettingsState = {
      var state = selecting(.agents)
      state.agents.agents[2].connection = .connecting
      return state
    }()

    /// Codex not on this Mac, no project taught yet, and a Connect that failed.
    @MainActor static let settingsAgentsEmpty: SettingsState = {
      var state = selecting(.agents)
      state.agents.agents[1].connection = .note("Not installed")
      state.agents.projects = []
      state.agents.notice = SettingsNote(text: "Claude app: its settings file is not valid JSON.", isProblem: true)
      return state
    }()

    /// A check for updates under way.
    @MainActor static let settingsAboutChecking: SettingsState = {
      var state = selecting(.about)
      state.about.update = .checking
      return state
    }()

    /// The check found nothing newer.
    @MainActor static let settingsAboutUpToDate: SettingsState = {
      var state = selecting(.about)
      state.about.answer = "Atlas is up to date."
      return state
    }()

    /// The check found a newer version: Install asks first.
    @MainActor static let settingsAboutNewer: SettingsState = {
      var state = selecting(.about)
      state.about.update = .install
      state.about.answer = "Atlas 0.2.0 is available."
      return state
    }()

    /// The check did not answer.
    @MainActor static let settingsAboutFailed: SettingsState = {
      var state = selecting(.about)
      state.about.answer = "Atlas couldn't check for updates."
      return state
    }()

    /// Recover asked for from the Helper pane, with the main window's words.
    @MainActor static let settingsRecover: SettingsState = {
      var state = selecting(.helper)
      state.alert = OrbitAlertState(
        title: "Recover the helper?",
        message: "Atlas checks the helper and starts it again. No action runs twice.",
        buttons: [
          OrbitAlertButton(id: .confirm, title: "Recover", role: .action),
          OrbitAlertButton(id: .cancel, title: "Not Now", role: .cancel),
        ]
      )
      return state
    }()

    /// Log Out asked for from the Helper pane, with the main window's words.
    @MainActor static let settingsLogOut: SettingsState = {
      var state = selecting(.helper)
      state.alert = OrbitAlertState(
        title: "Log out the helper?",
        message: "Its apps quit, and agents can't use it until you log in to it again.",
        buttons: [
          OrbitAlertButton(id: .cancel, title: "Cancel", role: .safeCancel),
          OrbitAlertButton(id: .confirm, title: "Log Out", role: .destructive),
        ]
      )
      return state
    }()

    /// Forget asked for a project: it stays listed until the person confirms.
    @MainActor static let settingsForget: SettingsState = {
      var state = selecting(.agents)
      state.alert = OrbitAlertState(
        title: "Forget opie?",
        message:
          "Atlas takes its section out of the project's AGENTS.md and CLAUDE.md. Agents there stop testing in the helper.",
        buttons: [
          OrbitAlertButton(id: .cancel, title: "Cancel", role: .safeCancel),
          OrbitAlertButton(id: .confirm, title: "Forget", role: .destructive),
        ]
      )
      return state
    }()

    /// Install asked for the newer version found.
    @MainActor static let settingsInstall: SettingsState = {
      var state = settingsAboutNewer
      state.alert = OrbitAlertState(
        title: "Install Atlas 0.2.0?",
        message: "Atlas checks the release's signature and installs it, then restarts itself to finish.",
        buttons: [
          OrbitAlertButton(id: .confirm, title: "Install", role: .action),
          OrbitAlertButton(id: .cancel, title: "Not Now", role: .cancel),
        ]
      )
      return state
    }()

    /// Switch asked for from the Helper pane, with the main window's words.
    @MainActor static let settingsSwitch: SettingsState = {
      var state = selecting(.helper)
      state.alert = OrbitAlertState(
        title: "Switch to the helper?",
        message:
          "Your screen locks so you can log in to the helper. Everything here keeps running, and you can switch back any time.",
        buttons: [
          OrbitAlertButton(id: .confirm, title: "Switch", role: .action),
          OrbitAlertButton(id: .cancel, title: "Cancel", role: .cancel),
        ]
      )
      return state
    }()

    /// An app in the helper held the log-out up: Log Out Anyway asks again.
    @MainActor static let settingsLogOutAnyway: SettingsState = {
      var state = selecting(.helper)
      state.alert = OrbitAlertState(
        title: "The helper didn't log out",
        message:
          "An app in the helper is holding it up. Log Out Anyway quits it, and anything unsaved there is lost.",
        buttons: [
          OrbitAlertButton(id: .cancel, title: "Cancel", role: .safeCancel),
          OrbitAlertButton(id: .confirm, title: "Log Out Anyway", role: .destructive),
        ]
      )
      return state
    }()

    /// Recover running from its alert: every button waits.
    @MainActor static let settingsRecovering: SettingsState = {
      var state = settingsRecover
      state.alert?.buttons[0].title = "Recovering…"
      state.alert?.isBusy = true
      state.helper.buttons.canRecover = false
      return state
    }()

    /// The newer version installing.
    @MainActor static let settingsAboutInstalling: SettingsState = {
      var state = selecting(.about)
      state.about.update = .installing
      state.about.answer = "Atlas 0.2.0 is available."
      return state
    }()

    /// The install failed with a long reason, which wraps under the buttons.
    @MainActor static let settingsAboutInstallFailed: SettingsState = {
      var state = selecting(.about)
      state.about.answer =
        "Atlas.app must be where the helper account can run it: install it in /Applications, then check for updates "
        + "again. The release was downloaded and its signature checked, but nothing was replaced."
      return state
    }()

    /// No release is published yet: a plain answer, not a failure.
    @MainActor static let settingsAboutUnpublished: SettingsState = {
      var state = selecting(.about)
      state.about.answer = "Atlas has no public release yet. Check atlas.insanearts.io for news."
      return state
    }()

    /// The board's General sample with another pane selected.
    ///
    /// - Parameter tab: The pane to show.
    /// - Returns: The sample.
    @MainActor private static func selecting(_ tab: SettingsTab) -> SettingsState {
      var state = settingsGeneral
      state.tabs.selected = tab
      return state
    }
  }
#endif
