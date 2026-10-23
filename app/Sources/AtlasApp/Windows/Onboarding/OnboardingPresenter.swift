import AtlasKit
import AtlasUI

/// Builds the setup window's state from what its controller holds. Pure: every choice the window shows is made here,
/// from the snapshot and AtlasKit's rules (the step rule, ``PasswordRule``, the switch progress and ``Permission``).
enum OnboardingPresenter {
  /// The window as it should look now.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: Everything the window draws.
  static func state(_ snapshot: OnboardingSnapshot) -> OnboardingState {
    let step = snapshot.step
    let content = content(snapshot, step: step)
    let footer: OnboardingFooterState =
      switch content {
      case .screen: footer(snapshot, step: step)
      case .loading, .permissionPanel: OnboardingFooterState()
      }
    return OnboardingState(
      content: content,
      footer: footer,
      note: note(snapshot),
      isBusy: snapshot.working != nil
    )
  }

  /// The permission panel while a permission is allowed, the first reading until it lands, or the step's screen.
  ///
  /// - Parameters:
  ///   - snapshot: What the controller holds.
  ///   - step: The screen the facts and choices lead to.
  /// - Returns: What fills the window.
  private static func content(_ snapshot: OnboardingSnapshot, step: OnboardingStep) -> OnboardingContent {
    if let permission = snapshot.guiding {
      return .permissionPanel(panel(permission, snapshot: snapshot))
    }
    guard snapshot.loaded else { return .loading(problem: snapshot.problem) }
    return .screen(screen(snapshot, step: step))
  }

  /// The step's screen and what it shows.
  ///
  /// - Parameters:
  ///   - snapshot: What the controller holds.
  ///   - step: The screen the facts and choices lead to.
  /// - Returns: The screen.
  private static func screen(_ snapshot: OnboardingSnapshot, step: OnboardingStep) -> OnboardingScreen {
    switch step {
    case .welcome: .welcome
    case .chooseHelper: .choose(choose(snapshot))
    case .createAccount: .create(create(snapshot))
    case .switchToHelper: .switchToHelper(switchToHelper(snapshot))
    case .finishing: .finishing(finishing(snapshot))
    case .agents: .agents(agents(snapshot))
    case .done: .done(done(snapshot))
    case .allow: .allow(allow(snapshot.facts))
    case .permissions: .permissions(permissions(snapshot))
    case .helperDone: .helperDone(helperDone(snapshot.facts.asker))
    case .blocked(let reason): .blocked(reason)
    }
  }

  /// The footer's buttons and dots. Back shows on the account choice (before a helper is set up), on the form and on
  /// Done, and Choose Another Account in its place on the switch while the helper asked never answered; the next
  /// step's button waits while a command runs, and each also for its own condition: an account picked, a form that
  /// can be sent, a passed check, the agents read or skipped. The finishing check's Continue is the one that Return
  /// does not press.
  ///
  /// - Parameters:
  ///   - snapshot: What the controller holds.
  ///   - step: The screen the facts and choices lead to.
  /// - Returns: The footer.
  private static func footer(_ snapshot: OnboardingSnapshot, step: OnboardingStep) -> OnboardingFooterState {
    let idle = snapshot.working == nil
    let back = OnboardingFooterAction(title: .back, event: .back, isEnabled: idle, isDefault: false)
    let next: OnboardingFooterAction? =
      switch step {
      case .chooseHelper:
        OnboardingFooterAction(
          title: .continue,
          event: .continueChoice,
          isEnabled: snapshot.shownPick != nil && idle,
          isDefault: true
        )
      case .createAccount:
        OnboardingFooterAction(
          title: .createAccount,
          event: .createAccount,
          isEnabled: snapshot.form.canCreate && idle,
          isDefault: true
        )
      case .finishing:
        OnboardingFooterAction(
          title: .continue,
          event: .continueFinishing,
          isEnabled: snapshot.finish == .passed,
          isDefault: false
        )
      case .agents:
        OnboardingFooterAction(
          title: .continue,
          event: .connectAgents,
          isEnabled: idle && !snapshot.agentsInstalling && (snapshot.agentsLoaded || snapshot.agentsFailed),
          isDefault: true
        )
      case .done:
        OnboardingFooterAction(title: .done, event: .complete, isEnabled: idle, isDefault: true)
      case .welcome, .switchToHelper, .allow, .permissions, .helperDone, .blocked:
        nil
      }
    let hasBack =
      switch step {
      case .chooseHelper: !snapshot.facts.configured
      case .createAccount, .done: true
      case .welcome, .switchToHelper, .finishing, .agents, .allow, .permissions, .helperDone, .blocked: false
      }
    // A helper asked that never answered can be let go, to choose another: in Back's place.
    let chooseAnother = OnboardingFooterAction(
      title: .chooseAnother,
      event: .chooseAnother,
      isEnabled: idle,
      isDefault: false
    )
    let left =
      if step == .switchToHelper, snapshot.facts.helper.forgettable {
        chooseAnother
      } else {
        hasBack ? back : nil
      }
    return OnboardingFooterState(back: left, next: next, dots: step.dots)
  }

  /// The line under a screen's controls: what runs, else what went wrong, else nothing.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The work note.
  static func note(_ snapshot: OnboardingSnapshot) -> WorkNote {
    if let working = snapshot.working { return .working(working) }
    if let problem = snapshot.problem { return .problem(problem) }
    return .none
  }
}
