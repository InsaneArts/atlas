extension OnboardingStep {
  /// The screen to show, decided from the facts and the person's choices
  /// alone, so a window opened half-way, or reopened, lands right.
  ///
  /// - Parameters:
  ///   - facts: What `atlas onboarding status` reported last.
  ///   - choices: What the person has chosen in the window.
  package init(facts: OnboardingFacts, choices: OnboardingChoices) {
    self = Self.step(facts, choices)
  }

  private static func step(_ facts: OnboardingFacts, _ choices: OnboardingChoices) -> OnboardingStep {
    if let problem = facts.appProblem { return .blocked(problem) }
    if facts.role == .helper {
      let asker = facts.asker
      if !asker.installed || !asker.running {
        // Asked by an account: allow it (again, over a job that does not
        // run, such as an old Atlas's). Asked by no one: nothing to allow here.
        return asker.name.isEmpty
          ? .blocked(
            "Atlas isn't running in this account. In your own account, open Atlas and choose this account as the helper again."
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
    if helper.leftoverSession {
      return .blocked(
        "macOS still runs a session of a deleted account with \(helper.title)'s user ID. Restart the Mac, then open Atlas again."
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
