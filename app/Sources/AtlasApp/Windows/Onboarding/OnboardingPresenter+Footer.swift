import AtlasOnboarding
import AtlasUI

/// The footer under every screen: Back, Not Now or Choose Another Account on the left; the track in the middle; the
/// screen's one main action on the right. One function per family of screens.
extension OnboardingPresenter {
  /// The footer's buttons and track. Back shows on the account choice (before a helper is set up), on the form and on
  /// Done, Choose Another Account in its place on the switch while the helper asked never answered, and Not Now beside
  /// Allow in the helper. Every screen hands its main action to the right, the permissions screen the next permission
  /// to allow. A button that starts a command waits while one runs, and each also for its own condition: an account
  /// picked, a form that can be sent, a passed check that has shown a moment, the agents read or skipped. Return
  /// presses the main action wherever it can be pressed.
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
    case .closeLeftovers: closeFooter(snapshot)
    case .allow: allowFooter(snapshot, step: step)
    case .welcome, .permissions, .helperDone, .blocked: oneActionFooter(snapshot, step: step)
    }
  }

  /// The footer under the first reading: Try Again once it failed, nothing while it runs.
  ///
  /// - Parameter failed: Whether the first reading failed.
  /// - Returns: The footer.
  static func loadingFooter(failed: Bool) -> OnboardingFooterState {
    guard failed else { return OnboardingFooterState() }
    return OnboardingFooterState(next: main(.tryAgain, .checkAgain, isEnabled: true))
  }

  /// The footer under the permission panel: Back to the permissions screen, at its bottom left; Escape presses it.
  static var panelFooter: OnboardingFooterState {
    OnboardingFooterState(
      back: OnboardingFooterAction(title: .back, event: .closePanel, isEnabled: true, isDefault: false)
    )
  }

  /// The welcome, the helper's permissions and done screens, and a blocked one: each one's own action. On the
  /// permissions screen it allows the permission whose turn it is, and there is none while the permissions cannot be
  /// read. Blocked by Atlas.app itself, which must move or be installed again before anything can pass, it shows the
  /// app in the Finder; blocked by anything else, it checks again.
  ///
  /// - Parameters:
  ///   - snapshot: What the controller holds.
  ///   - step: The welcome, a helper's screen or a blocked one.
  /// - Returns: The footer.
  private static func oneActionFooter(_ snapshot: OnboardingSnapshot, step: OnboardingStep) -> OnboardingFooterState {
    let idle = snapshot.working == nil
    let next: OnboardingFooterAction? =
      switch step {
      case .welcome: main(.getStarted, .begin, isEnabled: true)
      case .permissions:
        turn(snapshot)
          .map { allow($0, snapshot: snapshot, isEnabled: idle) }
      case .helperDone: main(.switchBack, .switchBack, isEnabled: idle && snapshot.isOnConsole)
      case .blocked where snapshot.facts.appProblem != nil: main(.showInFinder, .showInFinder, isEnabled: true)
      case .blocked: main(.checkAgain, .checkAgain, isEnabled: true)
      default: nil
      }
    return OnboardingFooterState(next: next, track: step.track)
  }

  /// The helper's allow screen: Allow, and Not Now beside it, which closes the window with nothing allowed.
  ///
  /// - Parameters:
  ///   - snapshot: What the controller holds.
  ///   - step: The allow screen.
  /// - Returns: The footer.
  private static func allowFooter(_ snapshot: OnboardingSnapshot, step: OnboardingStep) -> OnboardingFooterState {
    let idle = snapshot.working == nil
    return OnboardingFooterState(
      back: OnboardingFooterAction(title: .notNow, event: .notNow, isEnabled: idle, isDefault: false),
      next: main(.allow, .allow, isEnabled: idle),
      track: step.track
    )
  }

  /// The button that opens a permission's list in System Settings, named as the permission's row names it on this
  /// Mac.
  ///
  /// - Parameters:
  ///   - permission: The permission whose turn it is.
  ///   - snapshot: What the controller holds.
  ///   - isEnabled: Whether it can be pressed now.
  /// - Returns: The button.
  private static func allow(
    _ permission: Permission,
    snapshot: OnboardingSnapshot,
    isEnabled: Bool
  ) -> OnboardingFooterAction {
    main(.allowPermission(permission.title(on: snapshot.macOS)), .openSettings(id: permission.id), isEnabled: isEnabled)
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
      ? main(.createAccount, .createAccount, isEnabled: snapshot.form.canCreate && idle)
      : main(.continue, .continueChoice, isEnabled: snapshot.shownPick != nil && idle)
    let hasBack = creating || !snapshot.facts.configured
    return OnboardingFooterState(back: hasBack ? back(idle: idle) : nil, next: next, track: step.track)
  }

  /// The switch to the helper: Choose Another Account in Back's place while the helper asked never answered, and Lock
  /// and Switch until the helper is signed in on screen, after which the switch moves on by itself.
  ///
  /// - Parameters:
  ///   - snapshot: What the controller holds.
  ///   - step: The switch.
  /// - Returns: The footer.
  private static func switchFooter(_ snapshot: OnboardingSnapshot, step: OnboardingStep) -> OnboardingFooterState {
    let idle = snapshot.working == nil
    let helper = snapshot.facts.helper
    // A helper asked that never answered can be let go, to choose another: in Back's place.
    let chooseAnother = OnboardingFooterAction(
      title: .chooseAnother,
      event: .chooseAnother,
      isEnabled: idle,
      isDefault: false
    )
    let locks = !helper.signedIn || !helper.onConsole
    return OnboardingFooterState(
      back: helper.forgettable ? chooseAnother : nil,
      next: locks ? main(.lockAndSwitch, .lockAndSwitch, isEnabled: idle && snapshot.isOnConsole) : nil,
      track: step.track
    )
  }

  /// The check, the agents and Done: each one's Continue, Try Again after a failed check, or Done, and Back on Done
  /// alone.
  ///
  /// - Parameters:
  ///   - snapshot: What the controller holds.
  ///   - step: The check, the agents or Done.
  /// - Returns: The footer.
  private static func finishFooter(_ snapshot: OnboardingSnapshot, step: OnboardingStep) -> OnboardingFooterState {
    let idle = snapshot.working == nil
    switch step {
    case .finishing:
      if case .failed = snapshot.finish {
        return OnboardingFooterState(next: main(.tryAgain, .retryFinish, isEnabled: true), track: step.track)
      }
      // It lights up, and takes presses, Return's too, a moment after the check passes, so a press meant for the check
      // never skips past its result.
      let settled = snapshot.finish == .passed && snapshot.passSettled
      let next = OnboardingFooterAction(
        title: .continue,
        event: .continueFinishing,
        isEnabled: settled,
        isDefault: settled
      )
      return OnboardingFooterState(next: next, track: step.track)
    case .agents:
      let ready = idle && !snapshot.agentsInstalling && (snapshot.agentsLoaded || snapshot.agentsFailed)
      return OnboardingFooterState(next: main(.continue, .connectAgents, isEnabled: ready), track: step.track)
    default:
      // Done.
      return OnboardingFooterState(
        back: back(idle: idle),
        next: main(.done, .complete, isEnabled: idle),
        track: step.track
      )
    }
  }

  /// A screen's main action, which Return presses.
  ///
  /// - Parameters:
  ///   - title: Which button it is.
  ///   - event: What pressing it sends.
  ///   - isEnabled: Whether it can be pressed now.
  /// - Returns: The main action.
  private static func main(
    _ title: OnboardingFooterAction.Title,
    _ event: OnboardingEvent,
    isEnabled: Bool
  ) -> OnboardingFooterAction {
    OnboardingFooterAction(title: title, event: event, isEnabled: isEnabled, isDefault: true)
  }

  /// Back, which waits while a command runs.
  ///
  /// - Parameter idle: Whether no command runs.
  /// - Returns: Back.
  private static func back(idle: Bool) -> OnboardingFooterAction {
    OnboardingFooterAction(title: .back, event: .back, isEnabled: idle, isDefault: false)
  }
}
