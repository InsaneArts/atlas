#if DEBUG
  extension PreviewStates {
    /// Only Claude Code is installed and chosen.
    @MainActor public static let onboardingClaudeCode: OnboardingState = {
      var screen = onboardingAgentsScreen
      screen.agents[0].detail = "Test Mac apps from Claude Code"
      return OnboardingState(content: .screen(.agents(screen)), footer: onboardingAgents.footer, note: .none)
    }()

    /// Only the Claude app is installed and chosen.
    @MainActor public static let onboardingClaudeApp: OnboardingState = {
      var screen = onboardingAgentsScreen
      screen.agents[0].detail = "Live Mac view in the Claude app"
      return OnboardingState(content: .screen(.agents(screen)), footer: onboardingAgents.footer, note: .none)
    }()

    /// One Claude part failed, in the one row.
    @MainActor public static let onboardingClaudeFailure: OnboardingState = {
      var screen = onboardingAgentsScreen
      screen.agents[0].detail = "The Claude app: could not write settings"
      return OnboardingState(content: .screen(.agents(screen)), footer: onboardingAgents.footer, note: .none)
    }()
  }
#endif
