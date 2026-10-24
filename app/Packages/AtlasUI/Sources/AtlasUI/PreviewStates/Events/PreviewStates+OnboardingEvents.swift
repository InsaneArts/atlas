#if DEBUG
  extension PreviewStates {
    /// The setup window sample an event leads to, so the Interactive preview answers clicks as the app would: pick
    /// an account, switch agents, open and close the permission panel, and step between screens, back ones sliding
    /// the other way.
    ///
    /// - Parameters:
    ///   - state: The sample shown.
    ///   - event: What was clicked.
    /// - Returns: The sample to show next.
    @MainActor static func onboarding(_ state: OnboardingState, applying event: OnboardingEvent)
      -> OnboardingState
    {
      var next = state
      switch (state.content, event) {
      case (.screen(.choose(var screen)), .pick(let id)):
        screen.isNewAccountPicked = id == .newAccount
        for index in screen.accounts.indices {
          screen.accounts[index].isSelected = screen.accounts[index].id == id
        }
        next.content = .screen(.choose(screen))
      case (.screen(.choose(let screen)), .continueChoice):
        return screen.isNewAccountPicked ? onboardingCreate : onboardingSwitch
      case (.screen(.agents(var screen)), .setAgent(let id, let on)):
        for index in screen.agents.indices where screen.agents[index].id == id {
          screen.agents[index].isOn = on
        }
        next.content = .screen(.agents(screen))
      case (.screen(.done(var screen)), .setOpenAtLogin(let on)):
        screen.opensAtLogin = on
        next.content = .screen(.done(screen))
      case (.screen(.choose), .back):
        return wentBack(onboardingWelcome)
      case (.screen(.create), .back):
        return wentBack(onboardingChoose)
      case (.screen(.done), .back):
        return wentBack(onboardingAgents)
      case (_, .begin):
        return onboardingChoose
      case (_, .createAccount):
        return onboardingSwitch
      case (_, .connectAgents):
        return onboardingDone
      case (_, .allow), (_, .closePanel):
        return onboardingPermissions
      case (_, .openSettings):
        return onboardingPermissionPanel
      case (_, .retryFinish):
        return onboardingFinishing
      case (_, .lockAndSwitch):
        return onboardingSwitchOpenAtlas
      default:
        break
      }
      return next
    }

    /// A sample reached by going back, so it slides in the other way.
    ///
    /// - Parameter state: The sample.
    /// - Returns: The sample, moving back.
    @MainActor private static func wentBack(_ state: OnboardingState) -> OnboardingState {
      var state = state
      state.direction = .back
      return state
    }
  }
#endif
