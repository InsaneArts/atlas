#if DEBUG
  /// The setup window's permission samples: each permission as the helper's screens list it, in each of its
  /// standings, and the panel beside System Settings that guides one.
  extension PreviewStates {
    /// In the helper: the panel beside System Settings, Accessibility on and Screen Recording allowed now.
    @MainActor public static let onboardingPermissionPanel = OnboardingState(
      content: .permissionPanel(onboardingPermissionPanelState),
      footer: onboardingPanelFooter,
      note: .none
    )

    /// In the helper: the panel the moment Accessibility turns on, before Screen Recording's page opens by itself.
    @MainActor public static let onboardingPermissionPanelAllowed = OnboardingState(
      content: .permissionPanel(
        OnboardingPermissionPanelState(
          label: "1 of 2 · Accessibility",
          instruction: "Drag Atlas into the list.",
          alreadyListed: "Already in the list? Turn its switch on.",
          accessibility: onboardingAccessibilityOn,
          screenRecording: onboardingScreenRecordingOff,
          pill: "Allowed",
          isOn: true
        )
      ),
      footer: onboardingPanelFooter,
      note: .none
    )

    /// The panel's footer: Back alone, at its bottom left.
    @MainActor static let onboardingPanelFooter = OnboardingFooterState(
      back: OnboardingFooterAction(title: .back, event: .closePanel, isEnabled: true, isDefault: false)
    )

    /// Accessibility on, as macOS 26 names it.
    @MainActor static let onboardingAccessibilityOn = OnboardingPermissionItem(
      id: "accessibility",
      title: "Accessibility",
      isOn: true,
      status: "Allowed",
      allowTitle: "Allow…"
    )

    /// Accessibility off, the one to allow first.
    @MainActor static let onboardingAccessibilityOff = OnboardingPermissionItem(
      id: "accessibility",
      title: "Accessibility",
      isOn: false,
      status: "Off",
      allowTitle: "Allow…",
      canAllow: true
    )

    /// Screen Recording off, as macOS 26 names it, the one to allow next.
    @MainActor static let onboardingScreenRecordingOff = OnboardingPermissionItem(
      id: "screenRecording",
      title: "Screen & System Audio Recording",
      isOn: false,
      status: "Off",
      allowTitle: "Allow…",
      canAllow: true
    )

    /// Screen Recording off, waiting its turn behind Accessibility.
    @MainActor static let onboardingScreenRecordingWaiting = OnboardingPermissionItem(
      id: "screenRecording",
      title: "Screen & System Audio Recording",
      isOn: false,
      status: "Off",
      allowTitle: "Allow…"
    )

    /// A permission not read yet, while Atlas's job restarts.
    ///
    /// - Parameters:
    ///   - id: The permission's id.
    ///   - title: Its name.
    /// - Returns: The permission, being checked.
    @MainActor static func onboardingPermissionChecking(_ id: String, _ title: String) -> OnboardingPermissionItem {
      OnboardingPermissionItem(
        id: id,
        title: title,
        isOn: false,
        status: "Checking…",
        allowTitle: "Allow…",
        isKnown: false
      )
    }

    /// One permission in each standing a row shows: Allow…, Allowed, Checking… and Off.
    @MainActor static let onboardingPermissionSamples = [
      onboardingAccessibilityOff,
      OnboardingPermissionItem(id: "on", title: "Accessibility", isOn: true, status: "Allowed", allowTitle: "Allow…"),
      onboardingPermissionChecking("checking", "Screen & System Audio Recording"),
      OnboardingPermissionItem(
        id: "off",
        title: "Screen & System Audio Recording",
        isOn: false,
        status: "Off",
        allowTitle: "Allow…"
      ),
    ]

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
