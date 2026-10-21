#if DEBUG
  extension PreviewStates {
    /// Every setup window sample, in the order the person meets them.
    @MainActor package static var onboarding: [PreviewState<OnboardingState>] {
      [
        PreviewState(name: "Loading", value: onboardingLoading),
        PreviewState(name: "Loading, failed", value: onboardingLoadingFailed),
        PreviewState(name: "Welcome", value: onboardingWelcome),
        PreviewState(name: "Choose", value: onboardingChoose),
        PreviewState(name: "Choose, many accounts", value: onboardingChooseMany),
        PreviewState(name: "Choose, a problem", value: onboardingChooseProblem),
        PreviewState(name: "Choose, only the helper set up", value: onboardingChooseOnly),
        PreviewState(name: "Create", value: onboardingCreate),
        PreviewState(name: "Create, working", value: onboardingCreateWorking),
        PreviewState(name: "Switch", value: onboardingSwitch),
        PreviewState(name: "Switch, open Atlas", value: onboardingSwitchOpenAtlas),
        PreviewState(name: "Switch, a long name", value: onboardingSwitchLong),
        PreviewState(name: "Finishing", value: onboardingFinishing),
        PreviewState(name: "Finishing, passed", value: onboardingFinishingPassed),
        PreviewState(name: "Finishing, failed", value: onboardingFinishingFailed),
        PreviewState(name: "Agents", value: onboardingAgents),
        PreviewState(name: "Agents, not checked", value: onboardingAgentsNotChecked),
        PreviewState(name: "Done", value: onboardingDone),
        PreviewState(name: "Helper allow", value: onboardingAllow),
        PreviewState(name: "Helper allow, a long name", value: onboardingAllowLong),
        PreviewState(name: "Helper permissions", value: onboardingPermissions),
        PreviewState(name: "Helper guide", value: onboardingGuide),
        PreviewState(name: "Helper done", value: onboardingHelperDone),
        PreviewState(name: "Blocked", value: onboardingBlocked),
      ]
    }

    // MARK: Whole windows

    /// Before the first reading.
    @MainActor package static let onboardingLoading = OnboardingState(
      content: .loading(problem: nil),
      footer: OnboardingFooterState(),
      note: .none,
      isBusy: false
    )

    /// A fresh Mac: what Atlas is.
    @MainActor package static let onboardingWelcome = OnboardingState(
      content: .screen(.welcome),
      footer: OnboardingFooterState(back: nil, next: nil, dots: .init(index: 0, count: 6)),
      note: .none,
      isBusy: false
    )

    /// The account choice, a new account picked.
    @MainActor package static let onboardingChoose = OnboardingState(
      content: .screen(.choose(onboardingChooseScreen)),
      footer: OnboardingFooterState(
        back: onboardingBack,
        next: OnboardingFooterAction(title: .continue, event: .continueChoice, isEnabled: true, isDefault: true),
        dots: .init(index: 1, count: 6)
      ),
      note: .none,
      isBusy: false
    )

    /// The account form, its passwords not typed yet.
    @MainActor package static let onboardingCreate = OnboardingState(
      content: .screen(.create(onboardingCreateScreen)),
      footer: OnboardingFooterState(
        back: onboardingBack,
        next: OnboardingFooterAction(title: .createAccount, event: .createAccount, isEnabled: false, isDefault: true),
        dots: .init(index: 1, count: 6)
      ),
      note: .none,
      isBusy: false
    )

    /// The account being made: the form and the footer wait for macOS's dialog.
    @MainActor package static let onboardingCreateWorking: OnboardingState = {
      var state = onboardingCreate
      state.content = .screen(
        .create(
          OnboardingCreateScreenState(
            tail: "Confirm in the macOS dialog.",
            note: .working("Creating the account…"),
            isFormEnabled: false
          )
        )
      )
      state.footer.back?.isEnabled = false
      state.note = .working("Creating the account…")
      state.isBusy = true
      return state
    }()

    /// The helper chosen, not logged in yet: until it answers, another account can be chosen.
    @MainActor package static let onboardingSwitch = OnboardingState(
      content: .screen(.switchToHelper(onboardingSwitchScreen)),
      footer: OnboardingFooterState(
        back: OnboardingFooterAction(title: .chooseAnother, event: .chooseAnother, isEnabled: true, isDefault: false),
        next: nil,
        dots: .init(index: 2, count: 6)
      ),
      note: .none,
      isBusy: false
    )

    /// The helper logged in, off screen, with Atlas not open there yet.
    @MainActor package static let onboardingSwitchOpenAtlas: OnboardingState = {
      var state = onboardingSwitch
      state.content = .screen(
        .switchToHelper(
          OnboardingSwitchScreenState(
            title: "Now log in to the Atlas account.",
            logInDetail: "Choose the Atlas account on the lock screen",
            openAtlasTitle: "Open Atlas in the Atlas account",
            progress: .init(logIn: .done, openAtlas: .current, allow: .waiting),
            showsLockButton: true,
            waiting: "Next: Lock and Switch, then open Atlas there."
          )
        )
      )
      return state
    }()

    /// The end-to-end check under way.
    @MainActor package static let onboardingFinishing = OnboardingState(
      content: .screen(.finishing(onboardingFinishingScreen)),
      footer: OnboardingFooterState(
        back: nil,
        next: OnboardingFooterAction(title: .continue, event: .continueFinishing, isEnabled: false, isDefault: false),
        dots: .init(index: 3, count: 6)
      ),
      note: .none,
      isBusy: false
    )

    /// The check passed: Continue opens.
    @MainActor package static let onboardingFinishingPassed: OnboardingState = {
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
    @MainActor package static let onboardingFinishingFailed: OnboardingState = {
      var state = onboardingFinishing
      state.content = .screen(.finishing(onboardingFinishingFailedScreen))
      return state
    }()

    /// The agents, both found, before `atlas agents status` has answered, with one project chosen.
    @MainActor package static let onboardingAgents = OnboardingState(
      content: .screen(.agents(onboardingAgentsScreen)),
      footer: OnboardingFooterState(
        back: nil,
        next: OnboardingFooterAction(title: .continue, event: .connectAgents, isEnabled: false, isDefault: true),
        dots: .init(index: 4, count: 6)
      ),
      note: .none,
      isBusy: false
    )

    /// Setup done.
    @MainActor package static let onboardingDone = OnboardingState(
      content: .screen(.done(onboardingDoneScreen)),
      footer: OnboardingFooterState(
        back: onboardingBack,
        next: OnboardingFooterAction(title: .done, event: .complete, isEnabled: true, isDefault: true),
        dots: .init(index: 5, count: 6)
      ),
      note: .none,
      isBusy: false
    )

    /// In the helper: Ismayil asks to use it.
    @MainActor package static let onboardingAllow = OnboardingState(
      content: .screen(.allow(onboardingAllowScreen)),
      footer: OnboardingFooterState(back: nil, next: nil, dots: .init(index: 0, count: 3)),
      note: .none,
      isBusy: false
    )

    /// In the helper: Accessibility on, Screen Recording still off.
    @MainActor package static let onboardingPermissions = OnboardingState(
      content: .screen(.permissions(onboardingPermissionsScreen)),
      footer: OnboardingFooterState(back: nil, next: nil, dots: .init(index: 1, count: 3)),
      note: .none,
      isBusy: false
    )

    /// In the helper: the guide beside System Settings, waiting for Accessibility.
    @MainActor package static let onboardingGuide = OnboardingState(
      content: .guide(onboardingGuideState),
      footer: OnboardingFooterState(),
      note: .none,
      isBusy: false
    )

    /// In the helper: both permissions on.
    @MainActor package static let onboardingHelperDone = OnboardingState(
      content: .screen(.helperDone(onboardingHelperDoneScreen)),
      footer: OnboardingFooterState(back: nil, next: nil, dots: .init(index: 2, count: 3)),
      note: .none,
      isBusy: false
    )

    /// Atlas.app is not where it must be.
    @MainActor package static let onboardingBlocked = OnboardingState(
      content: .screen(.blocked(onboardingBlockedReason)),
      footer: OnboardingFooterState(),
      note: .none,
      isBusy: false
    )

    /// The first reading failed: why, and Try Again.
    @MainActor package static let onboardingLoadingFailed = OnboardingState(
      content: .loading(problem: onboardingLoadingProblem),
      footer: OnboardingFooterState(),
      note: .problem(onboardingLoadingProblem),
      isBusy: false
    )

    /// The account choice with more standard accounts than the window holds: the rows scroll, the footer stays.
    @MainActor package static let onboardingChooseMany: OnboardingState = {
      var state = onboardingChoose
      state.content = .screen(
        .choose(
          OnboardingChooseScreenState(
            isNewAccountPicked: true,
            accounts: [
              onboardingAccount("Test Account", "testaccount"),
              onboardingAccount("Guest Kid", "kid"),
              onboardingAccount("Maria Fernanda Montgomery-Castellanos de la Vega", "maria"),
              onboardingAccount("Studio", "studio"),
            ]
          )
        )
      )
      return state
    }()

    /// The account choice with two standard accounts, and why the last choice failed.
    @MainActor package static let onboardingChooseProblem: OnboardingState = {
      var state = onboardingChoose
      state.content = .screen(
        .choose(
          OnboardingChooseScreenState(
            isNewAccountPicked: true,
            accounts: [onboardingAccount("Test Account", "testaccount"), onboardingAccount("Guest Kid", "kid")]
          )
        )
      )
      state.note = .problem("macOS could not say whether kid is an administrator.")
      return state
    }()

    /// Atlas was set up in the account chosen before, so that account is the one choice.
    @MainActor package static let onboardingChooseOnly: OnboardingState = {
      var state = onboardingChoose
      state.content = .screen(
        .choose(
          OnboardingChooseScreenState(
            isNewAccountPicked: false,
            accounts: [
              OnboardingAccountChoice(
                id: .existing("atlas"),
                title: "Atlas",
                detail: "atlas · standard account on this Mac",
                mark: .letter("A"),
                isSelected: true
              )
            ],
            offersNewAccount: false
          )
        )
      )
      state.footer.back = nil
      return state
    }()

    /// A helper with a long name, never answered, and a switch that failed: Choose Another Account in Back's place.
    @MainActor package static let onboardingSwitchLong: OnboardingState = {
      var state = onboardingSwitch
      let name = "Maria Fernanda Montgomery-Castellanos de la Vega"
      state.content = .screen(
        .switchToHelper(
          OnboardingSwitchScreenState(
            title: "Now log in to \(name).",
            logInDetail: "Choose \(name) on the lock screen",
            openAtlasTitle: "Answer the welcome questions, then open Atlas there",
            progress: .init(logIn: .current, openAtlas: .waiting, allow: .waiting),
            showsLockButton: true,
            waiting: nil
          )
        )
      )
      state.note = .problem("The screen could not be locked: loginwindow did not answer within ten seconds.")
      return state
    }()

    /// `atlas agents status` failed: the agents listed unchecked, many projects chosen, and Continue skips the step.
    @MainActor package static let onboardingAgentsNotChecked: OnboardingState = {
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
            ],
            projects: onboardingAgentsManyProjects
          )
        )
      )
      state.footer.next?.isEnabled = true
      state.note = .problem("Atlas couldn't check your agents. Continue to skip this, or wait while it tries again.")
      return state
    }()

    /// In the helper: a person with a long name asks a helper with a long name, and allowing failed.
    @MainActor package static let onboardingAllowLong: OnboardingState = {
      var state = onboardingAllow
      let asker = "Christopher Alexander Montgomery-Smith"
      state.content = .screen(
        .allow(
          OnboardingAllowScreenState(
            initial: "C",
            eyebrow: "Atlas in Maria Fernanda Montgomery-Castellanos de la Vega",
            title: "Let \(asker) use this account",
            lede:
              "\(asker)'s agents will open apps here, click and type, while \(asker) keeps working in their own account. You allow this once; Atlas then starts here by itself."
          )
        )
      )
      state.note = .problem("Atlas could not finish setting up here: the login item could not be added.")
      return state
    }()

    // MARK: Pieces

    /// A standard account in the choice, not picked.
    ///
    /// - Parameters:
    ///   - title: Its full name.
    ///   - name: Its short name.
    /// - Returns: The account's row.
    @MainActor static func onboardingAccount(_ title: String, _ name: String) -> OnboardingAccountChoice {
      OnboardingAccountChoice(
        id: .existing(name),
        title: title,
        detail: "\(name) · standard account on this Mac",
        mark: .letter(String(title.prefix(1))),
        isSelected: false
      )
    }

    /// The form a new window opens with.
    @MainActor package static let onboardingForm = OnboardingAccountForm()

    /// Back, ready to press.
    @MainActor static let onboardingBack = OnboardingFooterAction(
      title: .back,
      event: .back,
      isEnabled: true,
      isDefault: false
    )

    /// A new account picked over one standard account.
    @MainActor static let onboardingChooseScreen = OnboardingChooseScreenState(
      isNewAccountPicked: true,
      accounts: [
        OnboardingAccountChoice(
          id: .existing("testaccount"),
          title: "Test Account",
          detail: "testaccount · standard account on this Mac",
          mark: .letter("T"),
          isSelected: false
        )
      ]
    )

    /// The form ready to fill.
    @MainActor static let onboardingCreateScreen = OnboardingCreateScreenState(
      tail: "macOS asks for your password next.",
      note: .none,
      isFormEnabled: true
    )

    /// The first of the three switch steps.
    @MainActor static let onboardingSwitchScreen = OnboardingSwitchScreenState(
      title: "Now log in to the Atlas account.",
      logInDetail: "Choose the Atlas account on the lock screen",
      openAtlasTitle: "Answer the welcome questions, then open Atlas there",
      progress: .init(logIn: .current, openAtlas: .waiting, allow: .waiting),
      showsLockButton: true,
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

    /// Claude Code and Codex found, and the Opie folder chosen.
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
      ],
      projects: [
        OnboardingProjectItem(
          id: "/Users/me/dev/Opie",
          name: "Opie",
          isTaught: false,
          isRemovable: true,
          removeLabel: "Remove Opie"
        )
      ]
    )

    /// Done, opening at login.
    @MainActor static let onboardingDoneScreen = OnboardingDoneScreenState(
      lede:
        "That was the whole setup. Atlas lives in your menu bar: ask your agent to check something on screen, and watch it work from there. After a restart, just log in to the Atlas account again; Atlas does the rest.",
      opensAtLogin: true
    )

    /// Ismayil asks the Atlas account.
    @MainActor static let onboardingAllowScreen = OnboardingAllowScreenState(
      initial: "I",
      eyebrow: "Atlas in the Atlas account",
      title: "Let Ismayil use this account",
      lede:
        "Ismayil's agents will open apps here, click and type, while Ismayil keeps working in their own account. You allow this once; Atlas then starts here by itself."
    )

    /// Accessibility on, Screen Recording next.
    @MainActor static let onboardingPermissionsScreen: OnboardingPermissionsScreenState = {
      let recording = OnboardingPermissionItem(
        id: "screenRecording",
        title: "Screen Recording",
        isOn: false,
        status: "Off",
        allowTitle: "Allow Screen Recording"
      )
      return OnboardingPermissionsScreenState(
        accessibility: OnboardingPermissionItem(
          id: "accessibility",
          title: "Accessibility",
          isOn: true,
          status: "Allowed",
          allowTitle: "Allow Accessibility"
        ),
        screenRecording: recording,
        next: recording
      )
    }()

    /// The guide waiting for Accessibility's switch.
    @MainActor static let onboardingGuideState = OnboardingGuideState(
      eyebrow: "1 of 2 · Accessibility",
      settingsPath: "Privacy & Security › Accessibility",
      turnOn: .current,
      comeBack: .waiting,
      pill: "Waiting for Accessibility…",
      isOn: false
    )

    /// Switching back to Ismayil.
    @MainActor static let onboardingHelperDoneScreen = OnboardingHelperDoneScreenState(tail: "Switch back to Ismayil.")

    /// What blocks a Mac whose Atlas.app is elsewhere.
    @MainActor static let onboardingBlockedReason =
      "Atlas.app must be in Applications, so the helper account can run it. Move it there, then open it again."

    /// Why the first reading failed, as the command said it.
    @MainActor package static let onboardingLoadingProblem =
      "Atlas could not run its command line at /Applications/Atlas.app/Contents/MacOS/atlas."

    /// Five chosen project folders, more than one row of chips holds.
    @MainActor static let onboardingAgentsManyProjects = [
      "Opie", "Jolt", "understudy", "markdown2pdf", "InsaneArts Website",
    ]
    .map { name in
      OnboardingProjectItem(
        id: "/Users/me/dev/\(name)",
        name: name,
        isTaught: false,
        isRemovable: true,
        removeLabel: "Remove \(name)"
      )
    }
  }
#endif
