#if DEBUG
  /// The setup window's samples in the person's account, from the first reading to Done: loading, the
  /// welcome, the switch, finishing, the agents and Done, with the screens they hold.
  extension PreviewStates {
    /// Before the first reading.
    @MainActor public static let onboardingLoading = OnboardingState(
      content: .loading(problem: nil),
      footer: OnboardingFooterState(),
      note: .none
    )

    /// A fresh Mac: what Atlas is.
    @MainActor public static let onboardingWelcome = OnboardingState(
      content: .screen(.welcome),
      footer: OnboardingFooterState(
        next: OnboardingFooterAction(title: .getStarted, event: .begin, isEnabled: true, isDefault: true),
        track: onboardingTrack(0, of: 6)
      ),
      note: .none
    )

    /// The helper chosen, not logged in yet: until it answers, another account can be chosen.
    @MainActor public static let onboardingSwitch = OnboardingState(
      content: .screen(.switchToHelper(onboardingSwitchScreen)),
      footer: OnboardingFooterState(
        back: OnboardingFooterAction(title: .chooseAnother, event: .chooseAnother, isEnabled: true, isDefault: false),
        next: OnboardingFooterAction(title: .lockAndSwitch, event: .lockAndSwitch, isEnabled: true, isDefault: true),
        track: onboardingTrack(2, of: 6)
      ),
      note: .none
    )

    /// The helper logged in, off screen, with Atlas not open there yet: the line says what Atlas waits for.
    @MainActor public static let onboardingSwitchOpenAtlas: OnboardingState = {
      var state = onboardingSwitch
      state.content = .screen(
        .switchToHelper(
          OnboardingSwitchScreenState(
            title: "Now log in to the Atlas account.",
            logInTitle: "Log in to the Atlas account",
            progress: .init(logIn: .done, openAtlas: .current, allow: .waiting),
            showsManualOpening: true,
            waiting: "Waiting for Atlas to open in the Atlas account…"
          )
        )
      )
      return state
    }()

    /// The end-to-end check under way.
    @MainActor public static let onboardingFinishing = OnboardingState(
      content: .screen(.finishing(onboardingFinishingScreen)),
      footer: OnboardingFooterState(
        next: OnboardingFooterAction(title: .continue, event: .continueFinishing, isEnabled: false, isDefault: false),
        track: onboardingTrack(3, of: 6)
      ),
      note: .none
    )

    /// The check passed: Continue opens, and Return presses it.
    @MainActor public static let onboardingFinishingPassed: OnboardingState = {
      var state = onboardingFinishing
      var screen = onboardingFinishingScreen
      screen.title = "Everything works."
      screen.tail = "Your agents have a Mac now."
      screen.live = .done
      screen.liveNote = "Passed"
      state.content = .screen(.finishing(screen))
      state.footer.next?.isEnabled = true
      state.footer.next?.isDefault = true
      return state
    }()

    /// The check failed: why, and Try Again.
    @MainActor public static let onboardingFinishingFailed: OnboardingState = {
      var state = onboardingFinishing
      state.content = .screen(.finishing(onboardingFinishingFailedScreen))
      state.footer.next = OnboardingFooterAction(
        title: .tryAgain,
        event: .retryFinish,
        isEnabled: true,
        isDefault: true
      )
      return state
    }()

    /// The agents, both found and switched on.
    @MainActor public static let onboardingAgents = OnboardingState(
      content: .screen(.agents(onboardingAgentsScreen)),
      footer: OnboardingFooterState(
        next: OnboardingFooterAction(title: .continue, event: .connectAgents, isEnabled: true, isDefault: true),
        track: onboardingTrack(4, of: 6)
      ),
      note: .none
    )

    /// Setup done.
    @MainActor public static let onboardingDone = OnboardingState(
      content: .screen(.done(onboardingDoneScreen)),
      footer: OnboardingFooterState(
        back: onboardingBack,
        next: OnboardingFooterAction(title: .done, event: .complete, isEnabled: true, isDefault: true),
        track: onboardingTrack(5, of: 6)
      ),
      note: .none
    )

    /// The first reading failed, Atlas's command line not running at all: why, in plain words, and Try Again.
    @MainActor public static let onboardingLoadingFailed = OnboardingState(
      content: .loading(problem: onboardingLoadingProblem),
      footer: OnboardingFooterState(
        next: OnboardingFooterAction(title: .tryAgain, event: .checkAgain, isEnabled: true, isDefault: true)
      ),
      note: .problem(onboardingLoadingProblem)
    )

    /// A helper with a long name, called by its given name, never answered, and a screen that didn't lock: Choose
    /// Another Account in Back's place.
    @MainActor public static let onboardingSwitchLong: OnboardingState = {
      var state = onboardingSwitch
      let name = "Maria Fernanda"
      state.content = .screen(
        .switchToHelper(
          OnboardingSwitchScreenState(
            title: "Now log in to \(name).",
            logInTitle: "Log in to \(name)",
            progress: .init(logIn: .current, openAtlas: .waiting, allow: .waiting),
            showsManualOpening: true,
            waiting: nil
          )
        )
      )
      state.note = .problem("The screen didn’t lock. Press Control-Command-Q, then choose \(name).")
      return state
    }()

    /// `atlas agents status` failed: the agents listed unchecked, and Continue skips the step.
    @MainActor public static let onboardingAgentsNotChecked: OnboardingState = {
      var state = onboardingAgents
      state.content = .screen(
        .agents(
          OnboardingAgentsScreenState(
            agents: [
              OnboardingAgentItem(
                id: "claude",
                title: "Claude",
                detail: "Not checked yet",
                icon: onboardingClaudeIcon,
                isOn: false,
                isEnabled: false
              ),
              OnboardingAgentItem(
                id: "codex",
                title: "Codex",
                detail: "Not checked yet",
                icon: onboardingCodexIcon,
                isOn: false,
                isEnabled: false
              ),
              OnboardingAgentItem(
                id: "opencode",
                title: "opencode",
                detail: "Not checked yet",
                icon: .mark(.opencode),
                isOn: false,
                isEnabled: false
              ),
              OnboardingAgentItem(
                id: "pi",
                title: "Pi",
                detail: "Not checked yet",
                icon: .mark(.pi),
                isOn: false,
                isEnabled: false
              ),
            ]
          )
        )
      )
      state.footer.next?.isEnabled = true
      state.note = .problem("Atlas couldn’t check your agents. Continue to skip this, or wait while it tries again.")
      return state
    }()

    /// Where a screen sits on the track, with what VoiceOver says for it.
    ///
    /// - Parameters:
    ///   - index: The screen, from 0.
    ///   - count: How many screens its side of setup has.
    /// - Returns: The track.
    @MainActor static func onboardingTrack(_ index: Int, of count: Int) -> OnboardingFooterState.Track {
      OnboardingFooterState.Track(index: index, count: count, label: "Step \(index + 1) of \(count)")
    }

    /// Back, ready to press.
    @MainActor static let onboardingBack = OnboardingFooterAction(
      title: .back,
      event: .back,
      isEnabled: true,
      isDefault: false
    )

    /// The first of the three switch steps, before Lock and Switch: no line under them.
    @MainActor static let onboardingSwitchScreen = OnboardingSwitchScreenState(
      title: "Now log in to the Atlas account.",
      logInTitle: "Log in to the Atlas account",
      progress: .init(logIn: .current, openAtlas: .waiting, allow: .waiting),
      showsManualOpening: true,
      waiting: nil
    )

    /// The checklist with its live check running.
    @MainActor static let onboardingFinishingScreen = OnboardingFinishingScreenState(
      title: "Checking everything",
      tail: "end to end.",
      loggedIn: "The Atlas account is logged in",
      permissionsNote: "Allowed",
      live: .busy,
      liveNote: "Checking…",
      failure: nil
    )

    /// The checklist with its live check failed.
    @MainActor static let onboardingFinishingFailedScreen: OnboardingFinishingScreenState = {
      var screen = onboardingFinishingScreen
      screen.title = "Almost there."
      screen.tail = "One check didn’t pass."
      screen.live = .failed
      screen.liveNote = "Failed"
      screen.failure = "The test screenshot never arrived."
      return screen
    }()

    /// Claude Code and Codex are available and switched on, so both get the Atlas skill.
    @MainActor static let onboardingAgentsScreen = OnboardingAgentsScreenState(
      agents: [
        OnboardingAgentItem(
          id: "claude",
          title: "Claude",
          detail: "Claude Code tools and a live Mac view in the app",
          icon: onboardingClaudeIcon,
          isOn: true,
          isEnabled: true
        ),
        OnboardingAgentItem(
          id: "codex",
          title: "Codex",
          detail: "Live Mac view in the Codex app",
          icon: onboardingCodexIcon,
          isOn: true,
          isEnabled: true
        ),
        OnboardingAgentItem(
          id: "opencode",
          title: "opencode",
          detail: "Test Mac apps on a hidden desktop",
          icon: .mark(.opencode),
          isOn: true,
          isEnabled: true
        ),
        OnboardingAgentItem(
          id: "pi",
          title: "Pi",
          detail: "Test Mac apps on a hidden desktop",
          icon: .mark(.pi),
          isOn: true,
          isEnabled: true
        ),
      ]
    )

    /// Claude Code's bundled original logo, as Settings draws it without an installed app.
    @MainActor static let onboardingClaudeIcon = RowIcon.app(AgentLogos.claude)

    /// Codex's bundled original logo, as Settings draws it without an installed app.
    @MainActor static let onboardingCodexIcon = RowIcon.app(AgentLogos.codex)

    /// Done, opening at login.
    @MainActor static let onboardingDoneScreen = OnboardingDoneScreenState(
      lede:
        "Atlas lives in your menu bar. Ask your agent to check something on screen and watch it work there. After a restart, log in to the Atlas account once and Atlas does the rest.",
      opensAtLogin: true
    )

    /// Why the first reading failed, when Atlas's command line could not run at all.
    @MainActor public static let onboardingLoadingProblem =
      "Atlas’s files look incomplete. Reinstall Atlas, then open it again."
  }
#endif
