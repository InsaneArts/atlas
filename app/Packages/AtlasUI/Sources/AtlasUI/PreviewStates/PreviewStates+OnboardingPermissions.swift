#if DEBUG
  /// The setup window's permission samples: each permission as the helper's screens list it, and the panel beside
  /// System Settings that guides one.
  extension PreviewStates {
    /// In the helper: the panel beside System Settings, Accessibility on and Screen Recording allowed now.
    @MainActor public static let onboardingPermissionPanel = OnboardingState(
      content: .permissionPanel(onboardingPermissionPanelState),
      footer: OnboardingFooterState(),
      note: .none,
      isBusy: false
    )

    /// Accessibility on, as macOS 26 names it.
    @MainActor static let onboardingAccessibilityOn = OnboardingPermissionItem(
      id: "accessibility",
      title: "Accessibility",
      isOn: true,
      status: "Allowed",
      allowTitle: "Allow…"
    )

    /// Screen Recording off, as macOS 26 names it.
    @MainActor static let onboardingScreenRecordingOff = OnboardingPermissionItem(
      id: "screenRecording",
      title: "Screen & System Audio Recording",
      isOn: false,
      status: "Off",
      allowTitle: "Allow…",
      canAllow: true
    )

    /// The panel while Screen Recording is allowed, Accessibility already on.
    @MainActor static let onboardingPermissionPanelState = OnboardingPermissionPanelState(
      label: "2 of 2 · Screen & System Audio Recording",
      instruction: "Drag Atlas into the top list.",
      alreadyListed: "Already in the top list? Turn its switch on.",
      accessibility: onboardingAccessibilityOn,
      screenRecording: onboardingScreenRecordingOff,
      pill: "Waiting for the switch…",
      isOn: false
    )
  }
#endif
