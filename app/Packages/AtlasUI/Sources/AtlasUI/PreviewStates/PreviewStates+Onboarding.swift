#if DEBUG
  extension PreviewStates {
    /// Every setup window sample, in the order the person meets them.
    @MainActor public static var onboarding: [PreviewState<OnboardingState>] {
      [
        PreviewState(name: "Loading", value: onboardingLoading),
        PreviewState(name: "Loading, failed", value: onboardingLoadingFailed),
        PreviewState(name: "Welcome", value: onboardingWelcome),
        PreviewState(name: "Choose", value: onboardingChoose),
        PreviewState(name: "Choose, many accounts", value: onboardingChooseMany),
        PreviewState(name: "Choose, a long name", value: onboardingChooseLong),
        PreviewState(name: "Choose, a problem", value: onboardingChooseProblem),
        PreviewState(name: "Choose, only the helper set up", value: onboardingChooseOnly),
        PreviewState(name: "Create", value: onboardingCreate),
        PreviewState(name: "Create, working", value: onboardingCreateWorking),
        PreviewState(name: "Create, a bad account name", value: onboardingCreateBadName),
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
        PreviewState(name: "Helper permission panel", value: onboardingPermissionPanel),
        PreviewState(name: "Helper done", value: onboardingHelperDone),
        PreviewState(name: "Blocked", value: onboardingBlocked),
      ]
    }
  }
#endif
