import AtlasOnboarding
import AtlasUI

/// Builds the setup window's state from what its controller holds. Pure: every choice the window shows is made here,
/// from the snapshot and AtlasOnboarding's rules (the step rule, ``PasswordRule``, the switch progress and
/// ``Permission``).
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
      case .loading(let problem): loadingFooter(failed: problem != nil)
      case .permissionPanel: panelFooter
      }
    return OnboardingState(
      content: content,
      footer: footer,
      note: note(snapshot),
      direction: snapshot.direction.page
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
