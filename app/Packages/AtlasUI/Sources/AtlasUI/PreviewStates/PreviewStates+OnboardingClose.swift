#if DEBUG
  extension PreviewStates {
    /// A deleted account's login is still open.
    @MainActor public static let onboardingClose = OnboardingState(
      content: .screen(.closeLeftovers(onboardingCloseScreen)),
      footer: OnboardingFooterState(
        next: OnboardingFooterAction(title: .closeIt, event: .closeLeftovers, isEnabled: true, isDefault: true)
      ),
      note: .none
    )

    /// Atlas closes it, under the person's approval.
    @MainActor public static let onboardingCloseClosing: OnboardingState = {
      var state = onboardingClose
      state.footer.next?.isEnabled = false
      state.note = .working("Closing…")
      return state
    }()

    /// The person cancelled macOS's approval.
    @MainActor public static let onboardingCloseCancelled: OnboardingState = {
      var state = onboardingClose
      state.note = .problem("It’s still open. Close it before you switch.")
      return state
    }()

    /// Closed: setup goes on.
    @MainActor public static let onboardingCloseClosed = OnboardingState(
      content: .screen(.closeLeftovers(onboardingClosedScreen)),
      footer: OnboardingFooterState(
        next: OnboardingFooterAction(title: .continue, event: .continueAfterClose, isEnabled: true, isDefault: true)
      ),
      note: .none
    )

    /// What the close screen says before it closes.
    @MainActor static let onboardingCloseScreen = OnboardingCloseScreenState(
      title: "A deleted account is still logged in",
      lede:
        "macOS kept a login open for an account that was deleted. Close it before you go on, or the next login can stall. macOS asks you to approve once.",
      isClosed: false
    )

    /// What it says once closed.
    @MainActor static let onboardingClosedScreen = OnboardingCloseScreenState(
      title: "Closed. You can go on.",
      lede: nil,
      isClosed: true
    )
  }
#endif
