import AtlasOnboarding
import AtlasUI

/// The choice of where agents work and the VM's path: the rows of the choice, the download, the install and the
/// permissions, each with why the add is held when it is, and their footers.
extension OnboardingPresenter {
  /// The choice: Second Login, VM and Cloud in order, the one picked filled, a kind this Mac cannot have now dimmed with
  /// why.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The choice's rows.
  static func desktop(_ snapshot: OnboardingSnapshot) -> OnboardingDesktopScreenState {
    let picked = snapshot.shownDesktop
    return OnboardingDesktopScreenState(
      items: DesktopChoice.all(snapshot.facts)
        .map { choice in
          OnboardingDesktopItem(
            id: choice.kind.shown,
            standing: choice.lack.map { .notYet(OnboardingWords.lack($0)) } ?? .choice(isPicked: choice.kind == picked)
          )
        }
    )
  }

  /// The download: how much of macOS is down, held in grey with why once the add stopped or failed.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The download's screen.
  static func vmDownload(_ snapshot: OnboardingSnapshot) -> OnboardingVMDownloadScreenState {
    let progress = snapshot.facts.vm?.progress
    let problem = vmProblem(snapshot)
    return OnboardingVMDownloadScreenState(
      fraction: progress?.downloaded,
      amount: OnboardingWords.downloaded(progress),
      isHeld: problem != nil,
      problem: problem
    )
  }

  /// Making the VM: its four steps as the add has done them, with why it is held.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The install's screen.
  static func vmInstall(_ snapshot: OnboardingSnapshot) -> OnboardingVMInstallScreenState {
    OnboardingVMInstallScreenState(
      steps: (snapshot.facts.vm ?? VMFacts()).makingSteps.map(\.stepState),
      problem: vmProblem(snapshot)
    )
  }

  /// The permissions in the VM's window, as the guest says it allows them, with why the add is held.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The allow screen.
  static func vmAllow(_ snapshot: OnboardingSnapshot) -> OnboardingVMAllowScreenState {
    OnboardingVMAllowScreenState(
      steps: (snapshot.facts.vm ?? VMFacts()).allowSteps.map(\.stepState),
      problem: vmProblem(snapshot)
    )
  }

  /// Why the VM's add is held: its process ended, or it failed and why; nil while it goes on.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The line, or nil.
  static func vmProblem(_ snapshot: OnboardingSnapshot) -> String? {
    switch snapshot.vmAdd {
    case .running: nil
    case .stopped: OnboardingWords.vmStopped
    case .failed(let reason): OnboardingWords.vmFailed(reason)
    }
  }

  /// The choice's footer: Back to the welcome, and Continue, ready at once, as Second Login is always picked when
  /// nothing else is.
  ///
  /// - Parameters:
  ///   - snapshot: What the controller holds.
  ///   - step: The choice.
  /// - Returns: The footer.
  static func desktopFooter(_ snapshot: OnboardingSnapshot, step: OnboardingStep) -> OnboardingFooterState {
    let idle = snapshot.working == nil
    return OnboardingFooterState(
      back: OnboardingFooterAction(title: .back, event: .back, isEnabled: idle, isDefault: false),
      next: OnboardingFooterAction(title: .continue, event: .continueDesktop, isEnabled: idle, isDefault: true),
      track: step.track(onVM: snapshot.onVM)
    )
  }

  /// The VM's footer while its add works: Continue dimmed until the VM is made, Go On once its process ended, Try
  /// Again once it failed. Back goes to the choice only from a download before anything of the VM was made.
  ///
  /// - Parameters:
  ///   - snapshot: What the controller holds.
  ///   - step: The download, the install or the permissions.
  /// - Returns: The footer.
  static func vmFooter(_ snapshot: OnboardingSnapshot, step: OnboardingStep) -> OnboardingFooterState {
    let add = snapshot.vmAdd
    let next =
      switch add {
      case .running:
        OnboardingFooterAction(title: .continue, event: .continueFinishing, isEnabled: false, isDefault: false)
      case .stopped: OnboardingFooterAction(title: .goOn, event: .goOnAdding, isEnabled: true, isDefault: true)
      case .failed: OnboardingFooterAction(title: .tryAgain, event: .retryAdding, isEnabled: true, isDefault: true)
      }
    let unmade = step == .downloadVM && snapshot.facts.vm == nil && add != .running
    return OnboardingFooterState(
      back: unmade ? OnboardingFooterAction(title: .back, event: .back, isEnabled: true, isDefault: false) : nil,
      next: next,
      track: step.track(onVM: true)
    )
  }
}
