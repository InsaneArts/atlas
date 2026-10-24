import AtlasOnboarding
import AtlasUI

/// The footer's buttons and dots, one function per family of screens.
extension OnboardingPresenter {
  /// The footer's buttons and dots. Back shows on the account choice (before a helper is set up), on the form and on
  /// Done, and Choose Another Account in its place on the switch while the helper asked never answered; the next
  /// step's button waits while a command runs, and each also for its own condition: an account picked, a form that
  /// can be sent, a passed check, the agents read or skipped. The finishing check's Continue is the one that Return
  /// does not press. The welcome and the helper's screens show the dots alone.
  ///
  /// - Parameters:
  ///   - snapshot: What the controller holds.
  ///   - step: The screen the facts and choices lead to.
  /// - Returns: The footer.
  static func footer(_ snapshot: OnboardingSnapshot, step: OnboardingStep) -> OnboardingFooterState {
    switch step {
    case .chooseHelper, .createAccount: accountFooter(snapshot, step: step)
    case .switchToHelper: switchFooter(snapshot, step: step)
    case .finishing, .agents, .done: finishFooter(snapshot, step: step)
    case .welcome, .allow, .permissions, .helperDone, .blocked: OnboardingFooterState(dots: step.dots)
    }
  }

  /// The account choice and the form: Continue or Create Account, and Back, on the choice only before a helper was
  /// set up.
  ///
  /// - Parameters:
  ///   - snapshot: What the controller holds.
  ///   - step: The choice or the form.
  /// - Returns: The footer.
  private static func accountFooter(_ snapshot: OnboardingSnapshot, step: OnboardingStep) -> OnboardingFooterState {
    let idle = snapshot.working == nil
    let creating = step == .createAccount
    let next =
      creating
      ? OnboardingFooterAction(
        title: .createAccount,
        event: .createAccount,
        isEnabled: snapshot.form.canCreate && idle,
        isDefault: true
      )
      : OnboardingFooterAction(
        title: .continue,
        event: .continueChoice,
        isEnabled: snapshot.shownPick != nil && idle,
        isDefault: true
      )
    let hasBack = creating || !snapshot.facts.configured
    return OnboardingFooterState(back: hasBack ? back(idle: idle) : nil, next: next, dots: step.dots)
  }

  /// The switch to the helper: Choose Another Account in Back's place while the helper asked never answered, and no
  /// next button, since the switch moves on by itself.
  ///
  /// - Parameters:
  ///   - snapshot: What the controller holds.
  ///   - step: The switch.
  /// - Returns: The footer.
  private static func switchFooter(_ snapshot: OnboardingSnapshot, step: OnboardingStep) -> OnboardingFooterState {
    // A helper asked that never answered can be let go, to choose another: in Back's place.
    let chooseAnother = OnboardingFooterAction(
      title: .chooseAnother,
      event: .chooseAnother,
      isEnabled: snapshot.working == nil,
      isDefault: false
    )
    return OnboardingFooterState(back: snapshot.facts.helper.forgettable ? chooseAnother : nil, dots: step.dots)
  }

  /// The check, the agents and Done: each one's Continue or Done, and Back on Done alone.
  ///
  /// - Parameters:
  ///   - snapshot: What the controller holds.
  ///   - step: The check, the agents or Done.
  /// - Returns: The footer.
  private static func finishFooter(_ snapshot: OnboardingSnapshot, step: OnboardingStep) -> OnboardingFooterState {
    let idle = snapshot.working == nil
    switch step {
    case .finishing:
      let next = OnboardingFooterAction(
        title: .continue,
        event: .continueFinishing,
        isEnabled: snapshot.finish == .passed,
        isDefault: false
      )
      return OnboardingFooterState(next: next, dots: step.dots)
    case .agents:
      let next = OnboardingFooterAction(
        title: .continue,
        event: .connectAgents,
        isEnabled: idle && !snapshot.agentsInstalling && (snapshot.agentsLoaded || snapshot.agentsFailed),
        isDefault: true
      )
      return OnboardingFooterState(next: next, dots: step.dots)
    default:
      // Done.
      let next = OnboardingFooterAction(title: .done, event: .complete, isEnabled: idle, isDefault: true)
      return OnboardingFooterState(back: back(idle: idle), next: next, dots: step.dots)
    }
  }

  /// Back, which waits while a command runs.
  ///
  /// - Parameter idle: Whether no command runs.
  /// - Returns: Back.
  private static func back(idle: Bool) -> OnboardingFooterAction {
    OnboardingFooterAction(title: .back, event: .back, isEnabled: idle, isDefault: false)
  }
}
