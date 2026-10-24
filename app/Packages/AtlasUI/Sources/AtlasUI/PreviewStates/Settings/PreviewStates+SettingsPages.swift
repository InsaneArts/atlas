#if DEBUG
  extension PreviewStates {
    /// Page migration samples, appended after every original scenario.
    @MainActor public static var settingsPages: [PreviewState<SettingsState>] {
      [
        PreviewState(name: "Screen live", value: screen(.live)),
        PreviewState(name: "Screen every five seconds", value: screen(.everyFiveSeconds)),
        PreviewState(name: "Screen paused", value: screen(.paused)),
        PreviewState(name: "Paused unavailable", value: screen(.live, canPause: false)),
        PreviewState(name: "Notifications on", value: notifications(true)),
        PreviewState(name: "Notifications off", value: notifications(false)),
        PreviewState(name: "Startup unavailable", value: startupUnavailable),
        PreviewState(name: "Diagnostics", value: page(.diagnostics)),
        PreviewState(name: "Diagnostics mixed", value: diagnosticsMixed),
        PreviewState(name: "Diagnostics empty", value: diagnosticsEmpty),
        PreviewState(name: "Diagnostics checking", value: diagnosticsChecking),
        PreviewState(name: "Projects", value: page(.projects)),
        PreviewState(name: "Projects empty", value: projectsEmpty),
        PreviewState(name: "Long and duplicate project names", value: projectsLong),
        PreviewState(name: "Many projects", value: projectsMany),
        PreviewState(name: "Forget failed", value: projectsFailed),
        PreviewState(name: "Long helper", value: settingsLongHelper),
        PreviewState(name: "Failed connection with Show File", value: settingsAgentsEmpty),
        PreviewState(name: "Partly connected Claude", value: settingsClaudePartial),
        PreviewState(name: "Update misplaced", value: updateMisplaced),
        PreviewState(name: "Update installed", value: updateInstalled),
        PreviewState(name: "Cleanup failed", value: cleanupFailed),
      ]
    }

    /// Selects a root or detail without performing an action.
    ///
    /// - Parameter destination: The page to draw.
    /// - Returns: The shared sample at that route.
    @MainActor static func page(_ destination: SettingsDestination) -> SettingsState {
      var state = settingsGeneral
      state.destination = destination
      return state
    }

    /// A cadence selection with the same three choices as the app.
    ///
    /// - Parameters:
    ///   - picture: The selected cadence.
    ///   - canPause: Whether the main window is open.
    /// - Returns: The screen page.
    @MainActor static func screen(_ picture: SettingsPicture, canPause: Bool = true) -> SettingsState {
      var state = page(.root(.screen))
      state.general.picture = picture
      state.general.pictureOptions[2].isEnabled = canPause || picture == .paused
      return state
    }

    /// A notification preference without implying macOS permission.
    ///
    /// - Parameter on: The stored preference.
    /// - Returns: The notification page.
    @MainActor static func notifications(_ on: Bool) -> SettingsState {
      var state = page(.root(.notifications))
      state.general.notify = on
      return state
    }

    /// An unsigned development build cannot register at login.
    @MainActor static var startupUnavailable: SettingsState {
      var state = page(.root(.general))
      state.general.canOpenAtLogin = false
      state.general.opensAtLogin = false
      return state
    }

    /// Ready, unknown and failed checks, including an unfamiliar future service check.
    @MainActor static var diagnosticsMixed: SettingsState {
      var state = settingsHelperNotReady
      state.destination = .diagnostics
      state.helper.ready.append(
        SettingsReadyRow(
          item: ReadinessItem(name: "Future helper capability", text: "Not reported yet.", state: .unknown)
        )
      )
      return state
    }

    /// No reported checks must not claim readiness.
    @MainActor static var diagnosticsEmpty: SettingsState {
      var state = page(.diagnostics)
      state.helper.ready = []
      return state
    }

    /// Explicit checking remains shared when the detail is open.
    @MainActor static var diagnosticsChecking: SettingsState {
      var state = diagnosticsMixed
      state.helper.buttons.canCheck = false
      for index in state.helper.ready.indices { state.helper.ready[index].isChecking = true }
      return state
    }

    /// Projects remains reachable without any loaded projects.
    @MainActor static var projectsEmpty: SettingsState {
      var state = page(.projects)
      state.agents.projects = []
      return state
    }

    /// Long names and duplicate leaf names retain distinct path identities.
    @MainActor static var projectsLong: SettingsState {
      var state = page(.projects)
      let name = "An unusually long project folder name shared by two different workspaces"
      state.agents.projects = ["work", "archive"].map { parent in
        SettingsProjectRow(
          id: "/Users/me/\(parent)/\(name)",
          name: name,
          detail: "~/\(parent)/\(name)",
          forgetLabel: "Forget \(name)"
        )
      }
      return state
    }

    /// A long list uses the page's one scroll view.
    @MainActor static var projectsMany: SettingsState {
      var state = page(.projects)
      state.agents.projects = (1...40)
        .map { index in
          SettingsProjectRow(
            id: "/Users/me/dev/project-\(index)",
            name: "project-\(index)",
            detail: "~/dev/project-\(index)",
            forgetLabel: "Forget project-\(index)"
          )
        }
      return state
    }

    /// A failed Forget leaves the project and its notice visible in the detail.
    @MainActor static var projectsFailed: SettingsState {
      var state = page(.projects)
      state.agents.notice = SettingsNote(text: "AGENTS.md has an Atlas marker without its pair.", kind: .problem)
      return state
    }

    /// An update that requires moving the app retains Show in Finder.
    @MainActor static var updateMisplaced: SettingsState {
      var state = page(.root(.about))
      state.about.status = .misplaced("Move Atlas to the Applications folder, then try again.")
      return state
    }

    /// Installation completion retains its restart wording.
    @MainActor static var updateInstalled: SettingsState {
      var state = page(.root(.about))
      state.about.status = .done("Atlas is updated and restarts itself to finish.")
      return state
    }

    /// Cleanup refusal leaves the existing account card and action available.
    @MainActor static var cleanupFailed: SettingsState {
      var state = settingsHelperLeftover
      state.helper.notice = SettingsNote(text: "It's still open. Close it before you switch.", kind: .problem)
      return state
    }
  }
#endif
