import AtlasOnboarding
import AtlasUI
import Foundation

/// Everything the setup window shows, as one plain value: the controller holds it, and ``OnboardingPresenter`` reads
/// it.
struct OnboardingSnapshot {
  /// What `atlas onboarding status` reported last.
  var facts = OnboardingFacts()
  /// What the person has chosen in the window.
  var choices = OnboardingChoices()
  /// Whether the facts have been read once.
  var loaded = false
  /// The command running now, in words.
  var working: String?
  /// What went wrong last, in one line.
  var problem: String?
  /// The account picked as the helper: a new one or one already on this Mac, nil for none yet.
  var picked: OnboardingAccountPick?
  /// The new helper account's form.
  var form = OnboardingAccountForm()
  /// Where the end-to-end check stands.
  var finish = FinishCheck.idle
  /// Whether the passed check has shown for ``OnboardingController/continueFollows``, so its Continue lights up and
  /// takes presses (v3 S5).
  var passSettled = false
  /// The agents, as reported and switched.
  var agents: [AgentChoice] = []
  /// Whether `atlas agents status` has said which agents are installed.
  var agentsLoaded = false
  /// Whether an agents install runs now, from this window or Settings, so Continue waits for it.
  var agentsInstalling = false
  /// Whether `atlas agents status` failed last: the agents stay listed, unchecked, and the step can be skipped.
  var agentsFailed = false
  /// Whether Atlas opens at login once setup is done.
  var opensAtLogin = true
  /// Whether the helper chosen in this window was readied for its first login, so Atlas opens there by itself; false
  /// when this macOS could not ready it, and the person opens Atlas there.
  var helperReadied = true
  /// The permission allowed now in System Settings, with the panel beside it.
  var guiding: Permission?
  /// This Mac's macOS, which names the permissions' pages.
  var macOS = ProcessInfo.processInfo.operatingSystemVersion
  /// Which way the window last moved between screens, so the new screen slides in the way the person goes.
  var direction = StepDirection.forward

  /// The screen the facts and choices lead to.
  var step: OnboardingStep { OnboardingStep(facts: facts, choices: choices) }

  /// The account picked, while the choice shows it: never an account it does not list, nor a new one it does not
  /// offer.
  var shownPick: OnboardingAccountPick? {
    switch picked {
    case .newAccount: facts.offers("") ? .newAccount : nil
    case .existing(let name): facts.offers(name) ? .existing(name) : nil
    case nil: nil
    }
  }
}
