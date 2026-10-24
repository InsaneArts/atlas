extension OnboardingStep {
  /// The screen to show, decided from the facts and the person's choices
  /// alone, so a window opened half-way, or reopened, lands right.
  ///
  /// - Parameters:
  ///   - facts: What `atlas onboarding status` reported last.
  ///   - choices: What the person has chosen in the window.
  public init(facts: OnboardingFacts, choices: OnboardingChoices) {
    self = Self.step(facts, choices)
  }

  private static func step(_ facts: OnboardingFacts, _ choices: OnboardingChoices) -> OnboardingStep {
    // Creation and preparation close leftover logins inside their own approval. Standalone cleanup is only
    // needed when setup can continue with the configured helper without choosing or creating an account.
    let needsChoice = !facts.configured || !facts.helper.exists || (!facts.helper.paired && !facts.helper.answering)
    if facts.role == .controller,
      choices.closedLeftovers || (!needsChoice && !facts.leftoverLogins.isEmpty)
    {
      return .closeLeftovers
    }
    if let problem = facts.appProblem { return .blocked(problem) }
    if let problem = facts.setupProblem { return .blocked(problem) }
    if facts.role == .helper {
      let asker = facts.asker
      if !asker.installed || !asker.running {
        if asker.accepted {
          return .blocked(
            "Allowed. Keep Atlas open in your own account to finish setup. If setup was interrupted, reopen Atlas here, then return to your own account to retry."
          )
        }
        // Asked by an account: allow it (again, over a job that does not
        // run, such as an old Atlas's). Asked by no one: nothing to allow here.
        return asker.name.isEmpty
          ? .blocked(
            "Atlas isn’t running in this account. In your own account, open Atlas and choose this account as the helper account again."
          )
          : .allow
      }
      if !asker.accessibility || !asker.screenCapture { return .permissions }
      return .helperDone
    }
    if !facts.configured || !facts.helper.exists {
      if !choices.started && !facts.configured { return .welcome }
      return choices.creating ? .createAccount : .chooseHelper
    }
    let helper = facts.helper
    if helper.administrator == true {
      return .blocked(
        "\(helper.title) administers this Mac. Make it a standard account in System Settings › Users & Groups, or choose another account."
      )
    }
    if facts.finished { return choices.agentsDone ? .done : .agents }
    // Chosen, but the helper was never asked (cut short): choose again.
    if !helper.paired && !helper.answering {
      return choices.creating ? .createAccount : .chooseHelper
    }
    let allowed = helper.accessibility && helper.screenCapture
    if helper.answering && allowed && helper.hidden { return .finishing }
    return .switchToHelper
  }
}
