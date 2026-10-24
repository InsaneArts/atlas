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

    /// In the helper: the panel the moment Accessibility turns on, before Screen Recording's page opens by itself:
    /// the header, the steps and the status still about Accessibility, Screen Recording not lit yet.
    @MainActor public static let onboardingPermissionPanelAllowed = OnboardingState(
      content: .permissionPanel(
        OnboardingPermissionPanelState(
          label: "1 of 2 · Accessibility",
          instruction: "Drag Atlas into the list.",
          alreadyListed: "Already in the list? Turn its switch on.",
          accessibility: onboardingAccessibilityOn,
          screenRecording: onboardingScreenRecordingTurn(isCurrent: false),
          status: .allowed("Allowed")
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
      standing: .allowed("Allowed")
    )

    /// Accessibility off, the one to allow first: its row says nothing, as the footer offers it.
    @MainActor static let onboardingAccessibilityTurn = OnboardingPermissionItem(
      id: "accessibility",
      title: "Accessibility",
      isOn: false,
      standing: .yourTurn,
      isCurrent: true
    )

    /// Screen Recording off, as macOS 26 names it, its turn come: its row says nothing.
    ///
    /// - Parameter isCurrent: Whether it is the one allowed now.
    /// - Returns: The permission.
    @MainActor static func onboardingScreenRecordingTurn(isCurrent: Bool) -> OnboardingPermissionItem {
      OnboardingPermissionItem(
        id: "screenRecording",
        title: "Screen & System Audio Recording",
        isOn: false,
        standing: .yourTurn,
        isCurrent: isCurrent
      )
    }

    /// Screen Recording off, waiting its turn behind Accessibility.
    @MainActor static let onboardingScreenRecordingNext = OnboardingPermissionItem(
      id: "screenRecording",
      title: "Screen & System Audio Recording",
      isOn: false,
      standing: .next("Next")
    )

    /// A permission not read yet, while Atlas's job restarts.
    ///
    /// - Parameters:
    ///   - id: The permission's id.
    ///   - title: Its name.
    /// - Returns: The permission, being checked.
    @MainActor static func onboardingPermissionChecking(_ id: String, _ title: String) -> OnboardingPermissionItem {
      OnboardingPermissionItem(id: id, title: title, isOn: false, standing: .checking("Checking…"), isKnown: false)
    }

    /// One permission in each standing a row shows: Allowed, Checking…, Next and Turned off.
    @MainActor static let onboardingPermissionSamples = [
      onboardingAccessibilityOn,
      onboardingPermissionChecking("checking", "Screen & System Audio Recording"),
      onboardingScreenRecordingNext,
      OnboardingPermissionItem(
        id: "turnedOff",
        title: "Accessibility",
        isOn: false,
        standing: .turnedOff("Turned off"),
        isCurrent: true
      ),
    ]

    /// The panel while Screen Recording is allowed, Accessibility already on.
    @MainActor static let onboardingPermissionPanelState = OnboardingPermissionPanelState(
      label: "2 of 2 · Screen & System Audio Recording",
      instruction: "Drag Atlas into the top list.",
      alreadyListed: "Already in the top list? Turn its switch on.",
      accessibility: onboardingAccessibilityOn,
      screenRecording: onboardingScreenRecordingTurn(isCurrent: true),
      status: .waiting("Waiting for you…")
    )
  }
#endif
