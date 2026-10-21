import AtlasKit
import AtlasUI

extension OnboardingPresenter {
  /// The account choice: a new account first, then each standard account, the picked one selected.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The choice's rows.
  static func choose(_ snapshot: OnboardingSnapshot) -> OnboardingChooseScreenState {
    OnboardingChooseScreenState(
      isNewAccountPicked: snapshot.shownPick == .newAccount,
      accounts: snapshot.facts.candidates.map { candidate in
        OnboardingAccountChoice(
          id: .existing(candidate.name),
          title: candidate.title,
          detail: OnboardingWords.candidateDetail(candidate.name),
          mark: .letter(candidate.title.initial),
          isSelected: snapshot.shownPick == .existing(candidate.name)
        )
      },
      offersNewAccount: !snapshot.facts.onlyChoice
    )
  }

  /// The account form: its tail says to confirm in macOS's dialog while the account is made, and a password problem
  /// takes the line under the form in place of the work note, which shows only once the password is right.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The form's screen.
  static func create(_ snapshot: OnboardingSnapshot) -> OnboardingCreateScreenState {
    let working = snapshot.working != nil
    return OnboardingCreateScreenState(
      tail: working ? OnboardingWords.confirmInDialog : OnboardingWords.passwordNext,
      note: PasswordRule.problem(password: snapshot.form.password, verify: snapshot.form.verify)
        .map(WorkNote.problem) ?? note(snapshot),
      isFormEnabled: !working
    )
  }

  /// The switch screen: the three steps from the helper's progress, the second's title once the helper has logged in,
  /// Lock and Switch until the helper is signed in on screen, and what happens next while nothing runs or failed.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The switch screen.
  static func switchToHelper(_ snapshot: OnboardingSnapshot) -> OnboardingSwitchScreenState {
    let helper = snapshot.facts.helper
    let progress = helper.switchProgress
    let quiet = snapshot.working == nil && snapshot.problem == nil
    return OnboardingSwitchScreenState(
      title: OnboardingWords.logIn(to: helper.title),
      logInDetail: OnboardingWords.chooseOnLockScreen(helper.title),
      openAtlasTitle: helper.signedIn ? OnboardingWords.openAtlas(in: helper.title) : OnboardingWords.answerWelcome,
      progress: OnboardingSwitchScreenState.Progress(
        logIn: progress.logIn.stepState,
        openAtlas: progress.openAtlas.stepState,
        allow: progress.allow.stepState
      ),
      showsLockButton: !helper.signedIn || !helper.onConsole,
      waiting: quiet ? waiting(helper, progress: progress) : nil
    )
  }

  /// What the switch screen waits for: the log-in, Atlas opening there, the two permissions, then the switch back.
  ///
  /// - Parameters:
  ///   - helper: The helper, as the person's account sees it.
  ///   - progress: The three steps' progress.
  /// - Returns: The pill's words.
  private static func waiting(_ helper: OnboardingFacts.Helper, progress: SwitchProgress) -> String {
    if !helper.signedIn { return OnboardingWords.lockThenLogIn(to: helper.title) }
    if !helper.answering {
      return helper.onConsole ? OnboardingWords.waitingForAtlas(in: helper.title) : OnboardingWords.lockThenOpenAtlas
    }
    if progress.allow != .done { return OnboardingWords.waitingForPermissions }
    return OnboardingWords.waitingForSwitchBack
  }

  /// The finishing check: checking, passed or failed, naming the helper logged in, with the failure over Try Again.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The finishing screen.
  static func finishing(_ snapshot: OnboardingSnapshot) -> OnboardingFinishingScreenState {
    let live: (state: CheckState, note: String, failure: String?) =
      switch snapshot.finish {
      case .passed: (.done, OnboardingWords.passed, nil)
      case .failed(let message): (.failed, OnboardingWords.failed, message)
      case .idle, .running: (.busy, OnboardingWords.checkingNote, nil)
      }
    let headline: (title: String, tail: String) =
      switch snapshot.finish {
      case .passed: (OnboardingWords.everythingWorks, OnboardingWords.agentsHaveAMac)
      case .failed: (OnboardingWords.almostThere, OnboardingWords.oneCheckFailed)
      case .idle, .running: (OnboardingWords.checking, OnboardingWords.endToEnd)
      }
    return OnboardingFinishingScreenState(
      title: headline.title,
      tail: headline.tail,
      loggedIn: OnboardingWords.loggedIn(snapshot.facts.helper.title),
      live: live.state,
      liveNote: live.note,
      failure: live.failure
    )
  }

  /// The agents and projects: an agent reads connected, found or not installed; its switch is on once connected or
  /// chosen, and only a found, unconnected one can be switched. A project can be removed until it is taught.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The agents screen.
  static func agents(_ snapshot: OnboardingSnapshot) -> OnboardingAgentsScreenState {
    OnboardingAgentsScreenState(
      agents: snapshot.agents.map { agent in
        let detail =
          if !agent.checked {
            snapshot.agentsFailed ? OnboardingWords.agentNotChecked : OnboardingWords.agentChecking
          } else if agent.connected {
            OnboardingWords.agentConnected
          } else if agent.found {
            OnboardingWords.agentFound
          } else {
            OnboardingWords.agentMissing
          }
        return OnboardingAgentItem(
          id: agent.id.rawValue,
          title: OnboardingWords.agentTitle(agent.id),
          detail: detail,
          isOn: agent.on || agent.connected,
          isEnabled: agent.found && !agent.connected
        )
      },
      projects: snapshot.projects.map { url in
        let taught = snapshot.taught.contains(url)
        return OnboardingProjectItem(
          id: url.path,
          name: url.lastPathComponent,
          isTaught: taught,
          isRemovable: !taught,
          removeLabel: OnboardingWords.remove(url.lastPathComponent)
        )
      }
    )
  }

  /// The done screen, naming the helper to log in to after a restart.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The done screen.
  static func done(_ snapshot: OnboardingSnapshot) -> OnboardingDoneScreenState {
    OnboardingDoneScreenState(
      lede: OnboardingWords.doneLede(snapshot.facts.helper.title),
      opensAtLogin: snapshot.opensAtLogin
    )
  }
}
