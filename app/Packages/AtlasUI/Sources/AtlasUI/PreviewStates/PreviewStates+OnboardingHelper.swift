#if DEBUG
  /// The setup window's samples in the helper's account: allowing the account that asked, the permissions, the
  /// helper's Done, and a Mac where setup is blocked.
  extension PreviewStates {
    /// In the helper: Ismayil asks to use it.
    @MainActor public static let onboardingAllow = OnboardingState(
      content: .screen(.allow(onboardingAllowScreen)),
      footer: OnboardingFooterState(
        back: OnboardingFooterAction(title: .notNow, event: .notNow, isEnabled: true, isDefault: false),
        next: OnboardingFooterAction(title: .allow, event: .allow, isEnabled: true, isDefault: true),
        track: onboardingTrack(0, of: 3)
      ),
      note: .none
    )

    /// In the helper: Accessibility on, Screen Recording's turn: the footer allows it.
    @MainActor public static let onboardingPermissions = OnboardingState(
      content: .screen(.permissions(onboardingPermissionsScreen)),
      footer: OnboardingFooterState(
        next: onboardingAllowPermission("screenRecording", "Screen & System Audio Recording"),
        track: onboardingTrack(1, of: 3)
      ),
      note: .none
    )

    /// In the helper: neither permission on yet, Accessibility first, Screen Recording next.
    @MainActor public static let onboardingPermissionsStart = OnboardingState(
      content: .screen(
        .permissions(
          OnboardingPermissionsScreenState(
            accessibility: onboardingAccessibilityTurn,
            screenRecording: onboardingScreenRecordingNext
          )
        )
      ),
      footer: OnboardingFooterState(
        next: onboardingAllowPermission("accessibility", "Accessibility"),
        track: onboardingTrack(1, of: 3)
      ),
      note: .none
    )

    /// The footer's button that allows a permission, named for it.
    ///
    /// - Parameters:
    ///   - id: The permission's id.
    ///   - title: Its name.
    /// - Returns: The button.
    @MainActor static func onboardingAllowPermission(_ id: String, _ title: String) -> OnboardingFooterAction {
      OnboardingFooterAction(
        title: .allowPermission(title),
        event: .openSettings(id: id),
        isEnabled: true,
        isDefault: true
      )
    }

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
      note: .none
    )

    /// In the helper: both permissions on.
    @MainActor public static let onboardingHelperDone = OnboardingState(
      content: .screen(.helperDone(onboardingHelperDoneScreen)),
      footer: OnboardingFooterState(
        next: OnboardingFooterAction(title: .switchBack, event: .switchBack, isEnabled: true, isDefault: true),
        track: onboardingTrack(2, of: 3)
      ),
      note: .none
    )

    /// Atlas.app is not where it must be: the footer shows it in the Finder.
    @MainActor public static let onboardingBlocked = OnboardingState(
      content: .screen(.blocked(onboardingBlockedReason)),
      footer: OnboardingFooterState(
        next: OnboardingFooterAction(title: .showInFinder, event: .showInFinder, isEnabled: true, isDefault: true)
      ),
      note: .none
    )

    /// In the helper: a person with a long name asks a helper with a long name, each called by their given name, and
    /// allowing failed.
    @MainActor public static let onboardingAllowLong: OnboardingState = {
      var state = onboardingAllow
      let asker = "Christopher"
      state.content = .screen(
        .allow(
          OnboardingAllowScreenState(
            initial: "C",
            label: "Atlas in Maria Fernanda",
            title: "Let \(asker) use this account",
            lede:
              "\(asker)’s agents will open apps here, click and type, while \(asker) keeps working in their own account. You allow this once; Atlas then starts here by itself."
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
        "Ismayil’s agents will open apps here, click and type, while Ismayil keeps working in their own account. You allow this once; Atlas then starts here by itself."
    )

    /// Accessibility on, Screen Recording's turn.
    @MainActor static let onboardingPermissionsScreen = OnboardingPermissionsScreenState(
      accessibility: onboardingAccessibilityOn,
      screenRecording: onboardingScreenRecordingTurn(isCurrent: true)
    )

    /// Switching back to Ismayil.
    @MainActor static let onboardingHelperDoneScreen = OnboardingHelperDoneScreenState(tail: "Switch back to Ismayil.")

    /// What blocks a Mac whose Atlas.app is elsewhere.
    @MainActor static let onboardingBlockedReason =
      "Atlas.app must be in Applications, so the helper account can run it. Move it there, then open it again."
  }
#endif
