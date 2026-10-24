#if DEBUG
  /// The setup window's samples in the helper's account: allowing the account that asked, the permissions, the
  /// helper's Done, and a Mac where setup is blocked.
  extension PreviewStates {
    /// In the helper: Ismayil asks to use it.
    @MainActor public static let onboardingAllow = OnboardingState(
      content: .screen(.allow(onboardingAllowScreen)),
      footer: OnboardingFooterState(
        next: OnboardingFooterAction(title: .allow, event: .allow, isEnabled: true, isDefault: true),
        track: onboardingTrack(0, of: 3)
      ),
      note: .none,
      isBusy: false
    )

    /// In the helper: Accessibility on, Screen Recording still off.
    @MainActor public static let onboardingPermissions = OnboardingState(
      content: .screen(.permissions(onboardingPermissionsScreen)),
      footer: OnboardingFooterState(track: onboardingTrack(1, of: 3)),
      note: .none,
      isBusy: false
    )

    /// In the helper: neither permission on yet, Accessibility first.
    @MainActor public static let onboardingPermissionsStart = OnboardingState(
      content: .screen(
        .permissions(
          OnboardingPermissionsScreenState(
            accessibility: onboardingAccessibilityOff,
            screenRecording: onboardingScreenRecordingWaiting
          )
        )
      ),
      footer: OnboardingFooterState(track: onboardingTrack(1, of: 3)),
      note: .none,
      isBusy: false
    )

    /// In the helper: both permissions being checked while Atlas's job restarts.
    @MainActor public static let onboardingPermissionsChecking = OnboardingState(
      content: .screen(
        .permissions(
          OnboardingPermissionsScreenState(
            accessibility: onboardingPermissionChecking("accessibility", "Accessibility"),
            screenRecording: onboardingPermissionChecking("screenRecording", "Screen & System Audio Recording")
          )
        )
      ),
      footer: OnboardingFooterState(track: onboardingTrack(1, of: 3)),
      note: .none,
      isBusy: false
    )

    /// In the helper: both permissions on.
    @MainActor public static let onboardingHelperDone = OnboardingState(
      content: .screen(.helperDone(onboardingHelperDoneScreen)),
      footer: OnboardingFooterState(
        next: OnboardingFooterAction(title: .switchBack, event: .switchBack, isEnabled: true, isDefault: true),
        track: onboardingTrack(2, of: 3)
      ),
      note: .none,
      isBusy: false
    )

    /// Atlas.app is not where it must be.
    @MainActor public static let onboardingBlocked = OnboardingState(
      content: .screen(.blocked(onboardingBlockedReason)),
      footer: OnboardingFooterState(
        next: OnboardingFooterAction(title: .checkAgain, event: .checkAgain, isEnabled: true, isDefault: true)
      ),
      note: .none,
      isBusy: false
    )

    /// In the helper: a person with a long name asks a helper with a long name, and allowing failed.
    @MainActor public static let onboardingAllowLong: OnboardingState = {
      var state = onboardingAllow
      let asker = "Christopher Alexander Montgomery-Smith"
      state.content = .screen(
        .allow(
          OnboardingAllowScreenState(
            initial: "C",
            label: "Atlas in Maria Fernanda Montgomery-Castellanos de la Vega",
            title: "Let \(asker) use this account",
            lede:
              "\(asker)'s agents will open apps here, click and type, while \(asker) keeps working in their own account. You allow this once; Atlas then starts here by itself."
          )
        )
      )
      state.note = .problem("Atlas could not finish setting up here: the login item could not be added.")
      return state
    }()

    /// Ismayil asks the Atlas account.
    @MainActor static let onboardingAllowScreen = OnboardingAllowScreenState(
      initial: "I",
      label: "Atlas in the Atlas account",
      title: "Let Ismayil use this account",
      lede:
        "Ismayil's agents will open apps here, click and type, while Ismayil keeps working in their own account. You allow this once; Atlas then starts here by itself."
    )

    /// Accessibility on, Screen Recording next.
    @MainActor static let onboardingPermissionsScreen = OnboardingPermissionsScreenState(
      accessibility: onboardingAccessibilityOn,
      screenRecording: onboardingScreenRecordingOff
    )

    /// Switching back to Ismayil.
    @MainActor static let onboardingHelperDoneScreen = OnboardingHelperDoneScreenState(tail: "Switch back to Ismayil.")

    /// What blocks a Mac whose Atlas.app is elsewhere.
    @MainActor static let onboardingBlockedReason =
      "Atlas.app must be in Applications, so the helper account can run it. Move it there, then open it again."
  }
#endif
