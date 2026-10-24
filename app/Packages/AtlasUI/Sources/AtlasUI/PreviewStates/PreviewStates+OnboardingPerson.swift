#if DEBUG
  /// The setup window's samples in the person's account, from the first reading to Done: loading, the
  /// welcome, the switch, finishing, the agents and Done, with the screens they hold.
  extension PreviewStates {
    /// Before the first reading.
    @MainActor public static let onboardingLoading = OnboardingState(
      content: .loading(problem: nil),
      footer: OnboardingFooterState(),
      note: .none,
      isBusy: false
    )

    /// A fresh Mac: what Atlas is.
    @MainActor public static let onboardingWelcome = OnboardingState(
      content: .screen(.welcome),
      footer: OnboardingFooterState(
        next: OnboardingFooterAction(title: .getStarted, event: .begin, isEnabled: true, isDefault: true),
        track: onboardingTrack(0, of: 6)
      ),
      note: .none,
      isBusy: false
    )

    /// The helper chosen, not logged in yet: until it answers, another account can be chosen.
    @MainActor public static let onboardingSwitch = OnboardingState(
      content: .screen(.switchToHelper(onboardingSwitchScreen)),
      footer: OnboardingFooterState(
        back: OnboardingFooterAction(title: .chooseAnother, event: .chooseAnother, isEnabled: true, isDefault: false),
        next: OnboardingFooterAction(title: .lockAndSwitch, event: .lockAndSwitch, isEnabled: true, isDefault: true),
        track: onboardingTrack(2, of: 6)
      ),
      note: .none,
      isBusy: false
    )

    /// The helper logged in, off screen, with Atlas not open there yet.
    @MainActor public static let onboardingSwitchOpenAtlas: OnboardingState = {
      var state = onboardingSwitch
      state.content = .screen(
        .switchToHelper(
          OnboardingSwitchScreenState(
            title: "Now log in to the Atlas account.",
            logInDetail: "Choose the Atlas account on the lock screen",
            progress: .init(logIn: .done, openAtlas: .current, allow: .waiting),
            waiting: "Next: Lock and Switch, then allow Atlas there."
          )
        )
      )
      return state
    }()

    /// The end-to-end check under way.
    @MainActor public static let onboardingFinishing = OnboardingState(
      content: .screen(.finishing(onboardingFinishingScreen)),
      footer: OnboardingFooterState(
        next: OnboardingFooterAction(
          title: .continue,
          event: .continueFinishing,
          isEnabled: false,
          isDefault: false,
          followsSuccess: true
        ),
        track: onboardingTrack(3, of: 6)
      ),
      note: .none,
      isBusy: false
    )

    /// The check passed: Continue opens.
    @MainActor public static let onboardingFinishingPassed: OnboardingState = {
      var state = onboardingFinishing
      var screen = onboardingFinishingScreen
      screen.title = "Everything works."
      screen.tail = "Your agents have a Mac now."
      screen.live = .done
      screen.liveNote = "passed"
      state.content = .screen(.finishing(screen))
      state.footer.next?.isEnabled = true
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

    /// The agents, both found, before `atlas agents status` has answered.
    @MainActor public static let onboardingAgents = OnboardingState(
      content: .screen(.agents(onboardingAgentsScreen)),
      footer: OnboardingFooterState(
        next: OnboardingFooterAction(title: .continue, event: .connectAgents, isEnabled: false, isDefault: true),
        track: onboardingTrack(4, of: 6)
      ),
      note: .none,
      isBusy: false
    )

    /// Setup done.
    @MainActor public static let onboardingDone = OnboardingState(
      content: .screen(.done(onboardingDoneScreen)),
      footer: OnboardingFooterState(
        back: onboardingBack,
        next: OnboardingFooterAction(title: .done, event: .complete, isEnabled: true, isDefault: true),
        track: onboardingTrack(5, of: 6)
      ),
      note: .none,
      isBusy: false
    )

    /// The first reading failed: why, and Try Again.
    @MainActor public static let onboardingLoadingFailed = OnboardingState(
      content: .loading(problem: onboardingLoadingProblem),
      footer: OnboardingFooterState(
        next: OnboardingFooterAction(title: .tryAgain, event: .checkAgain, isEnabled: true, isDefault: true)
      ),
      note: .problem(onboardingLoadingProblem),
      isBusy: false
    )

    /// A helper with a long name, never answered, and a switch that failed: Choose Another Account in Back's place.
    @MainActor public static let onboardingSwitchLong: OnboardingState = {
      var state = onboardingSwitch
      let name = "Maria Fernanda Montgomery-Castellanos de la Vega"
      state.content = .screen(
        .switchToHelper(
          OnboardingSwitchScreenState(
            title: "Now log in to \(name).",
            logInDetail: "Choose \(name) on the lock screen",
            progress: .init(logIn: .current, openAtlas: .waiting, allow: .waiting),
            waiting: nil
          )
        )
      )
      state.note = .problem("The screen could not be locked: loginwindow did not answer within ten seconds.")
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
                title: "Claude Code",
                detail: "Not checked yet.",
                isOn: false,
                isEnabled: false
              ),
              OnboardingAgentItem(
                id: "codex",
                title: "Codex",
                detail: "Not checked yet.",
                isOn: false,
                isEnabled: false
              ),
            ]
          )
        )
      )
      state.footer.next?.isEnabled = true
      state.note = .problem("Atlas couldn't check your agents. Continue to skip this, or wait while it tries again.")
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

    /// The first of the three switch steps.
    @MainActor static let onboardingSwitchScreen = OnboardingSwitchScreenState(
      title: "Now log in to the Atlas account.",
      logInDetail: "Choose the Atlas account on the lock screen",
      progress: .init(logIn: .current, openAtlas: .waiting, allow: .waiting),
      waiting: "Next: Lock and Switch, then log in to the Atlas account."
    )

    /// The checklist with its live check running.
    @MainActor static let onboardingFinishingScreen = OnboardingFinishingScreenState(
      title: "Checking everything",
      tail: "end to end.",
      loggedIn: "The Atlas account is logged in",
      live: .busy,
      liveNote: "checking…",
      failure: nil
    )

    /// The checklist with its live check failed.
    @MainActor static let onboardingFinishingFailedScreen: OnboardingFinishingScreenState = {
      var screen = onboardingFinishingScreen
      screen.title = "Almost there."
      screen.tail = "One check didn't pass."
      screen.live = .failed
      screen.liveNote = "failed"
      screen.failure = "The test screenshot never arrived."
      return screen
    }()

    /// Claude Code and Codex found, both switched on.
    @MainActor static let onboardingAgentsScreen = OnboardingAgentsScreenState(
      agents: [
        OnboardingAgentItem(
          id: "claude",
          title: "Claude Code",
          detail: "Found. Adds the Atlas skill to every session.",
          isOn: true,
          isEnabled: true
        ),
        OnboardingAgentItem(
          id: "codex",
          title: "Codex",
          detail: "Found. Adds the Atlas skill to every session.",
          isOn: true,
          isEnabled: true
        ),
      ]
    )

    /// Done, opening at login.
    @MainActor static let onboardingDoneScreen = OnboardingDoneScreenState(
      lede:
        "That was the whole setup. Atlas lives in your menu bar: ask your agent to check something on screen, and watch it work from there. After a restart, just log in to the Atlas account again; Atlas does the rest. Atlas doesn't need administrator access again. Removing it later asks once more, to stop it in the helper.",
      opensAtLogin: true
    )

    /// Why the first reading failed, as the command said it.
    @MainActor public static let onboardingLoadingProblem =
      "Atlas could not run its command line at /Applications/Atlas.app/Contents/MacOS/atlas."
  }
#endif
