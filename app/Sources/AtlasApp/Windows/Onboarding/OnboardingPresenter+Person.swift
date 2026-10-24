import AtlasOnboarding
import AtlasUI

extension OnboardingPresenter {
  /// The account choice: a new account first, then each standard account, the picked one selected, a problem setting
  /// one up in its own row, with what approving in macOS will do for an existing account picked (no approval comes for
  /// the account Atlas was set up in before).
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The choice's rows.
  static func choose(_ snapshot: OnboardingSnapshot) -> OnboardingChooseScreenState {
    let picksExisting = snapshot.facts.candidates.contains { snapshot.shownPick == .existing($0.name) }
    let problemAccount = problemAccount(snapshot)
    return OnboardingChooseScreenState(
      isNewAccountPicked: snapshot.shownPick == .newAccount,
      accounts: snapshot.facts.candidates.map { candidate in
        OnboardingAccountChoice(
          id: .existing(candidate.name),
          title: candidate.title,
          detail: OnboardingWords.candidateDetail(candidate.name),
          mark: .letter(candidate.title.initial),
          isSelected: snapshot.shownPick == .existing(candidate.name),
          problem: candidate.name == problemAccount ? snapshot.problem : nil
        )
      },
      offersNewAccount: !snapshot.facts.onlyChoice,
      approval: !snapshot.facts.onlyChoice && picksExisting ? OnboardingWords.approveToReady : nil
    )
  }

  /// The account form: its tail says macOS will ask to approve it once, and a problem with the short name, then with
  /// the password, takes the line under the form in place of the work note, which shows only once both are right.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The form's screen.
  static func create(_ snapshot: OnboardingSnapshot) -> OnboardingCreateScreenState {
    let working = snapshot.working != nil
    let form = snapshot.form
    let nameProblem = AccountNameRule.problem(form.accountName)
    let passwordProblem = PasswordRule.problem(password: form.password, verify: form.verify)
    // A password problem is the verify field's only when the two differ; the others are about the password itself.
    let differs = passwordProblem != nil && !form.verify.isEmpty && form.password != form.verify
    return OnboardingCreateScreenState(
      tail: OnboardingWords.approveOnce,
      note: (nameProblem ?? passwordProblem)
        .map(WorkNote.problem) ?? note(snapshot),
      isFormEnabled: !working,
      flagged: nameProblem != nil ? .accountName : differs ? .verify : nil
    )
  }

  /// The switch screen: the three steps from the helper's progress, and what happens next while nothing runs or
  /// failed. Lock and Switch is the footer's.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The switch screen.
  static func switchToHelper(_ snapshot: OnboardingSnapshot) -> OnboardingSwitchScreenState {
    let helper = snapshot.facts.helper
    let progress = helper.switchProgress
    let quiet = snapshot.working == nil && snapshot.problem == nil
    return OnboardingSwitchScreenState(
      title: OnboardingWords.logIn(to: helper.title),
      logInTitle: OnboardingWords.logInStep(to: helper.title),
      progress: OnboardingSwitchScreenState.Progress(
        logIn: progress.logIn.stepState,
        openAtlas: progress.openAtlas.stepState,
        allow: progress.allow.stepState
      ),
      opensByItself: snapshot.helperReadied,
      waiting: quiet ? waiting(helper, progress: progress) : nil
    )
  }

  /// What the switch screen waits for once the person has logged in to the helper: Atlas opening there, the two
  /// permissions, then the switch back. Before that, nothing: the steps and Lock and Switch already say what to do.
  ///
  /// - Parameters:
  ///   - helper: The helper, as the person's account sees it.
  ///   - progress: The three steps' progress.
  /// - Returns: The line's words, or nil for none.
  private static func waiting(_ helper: OnboardingFacts.Helper, progress: SwitchProgress) -> String? {
    if !helper.signedIn { return nil }
    if !helper.answering { return OnboardingWords.waitingForAtlas(in: helper.title) }
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
      case .passed: (.done, StatusWord.passed.text, nil)
      case .failed(let message): (.failed, StatusWord.failed.text, message)
      case .idle, .running: (.busy, StatusWord.checking.text, nil)
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
      permissionsNote: StatusWord.allowed.text,
      live: live.state,
      liveNote: live.note,
      failure: live.failure
    )
  }

  /// The agents, each under its icon as Settings shows it: Connected, Gets the Atlas skill while switched on, Found
  /// while switched off, or Not on this Mac; its switch is on once connected or chosen, and only a found, unconnected
  /// one can be switched.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The agents screen.
  static func agents(_ snapshot: OnboardingSnapshot) -> OnboardingAgentsScreenState {
    OnboardingAgentsScreenState(
      agents: snapshot.agents.map { agent in
        let detail =
          if !agent.checked {
            snapshot.agentsFailed ? OnboardingWords.agentNotChecked : StatusWord.checking.text
          } else if agent.connected {
            OnboardingWords.agentConnected
          } else if agent.found {
            agent.on ? OnboardingWords.agentGetsSkill : OnboardingWords.agentFound
          } else {
            OnboardingWords.agentMissing
          }
        return OnboardingAgentItem(
          id: agent.id.rawValue,
          title: OnboardingWords.agentTitle(agent.id),
          detail: detail,
          icon: AgentRowIcons.icon(agent.id.rawValue, snapshot.agentIcons),
          isOn: agent.on || agent.connected,
          isEnabled: agent.found && !agent.connected
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
