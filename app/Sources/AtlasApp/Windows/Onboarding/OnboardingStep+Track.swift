import AtlasOnboarding
import AtlasUI

extension OnboardingStep {
  /// Where the step sits on the track at the bottom of the window: seven segments on Second Login's path, where the
  /// form shares the choice's, eight on a VM's, three on the helper's; none while blocked.
  ///
  /// - Parameter onVM: Whether the person's side goes the VM's path: a VM picked, chosen or being added.
  /// - Returns: The track, or nil for none.
  func track(onVM: Bool) -> OnboardingFooterState.Track? {
    guard let place = place(onVM: onVM) else { return nil }
    return OnboardingFooterState.Track(
      index: place.index,
      count: place.count,
      label: OnboardingWords.step(place.index, of: place.count)
    )
  }

  /// The step's segment and how many segments its path has; nil while a login is closed or setup is blocked.
  ///
  /// - Parameter onVM: Whether the person's side goes the VM's path.
  /// - Returns: The segment and the count.
  private func place(onVM: Bool) -> (index: Int, count: Int)? {
    let person = onVM ? 8 : 7
    // The check, the agents and Done come after the VM's three steps or Second Login's two.
    let after = onVM ? 5 : 4
    switch self {
    case .welcome: return (0, person)
    case .chooseDesktop: return (1, person)
    case .chooseHelper, .createAccount: return (2, 7)
    case .switchToHelper: return (3, 7)
    case .downloadVM: return (2, 8)
    case .installVM: return (3, 8)
    case .allowVM: return (4, 8)
    case .finishing: return (after, person)
    case .agents: return (after + 1, person)
    case .done: return (after + 2, person)
    case .allow: return (0, 3)
    case .permissions: return (1, 3)
    case .helperDone: return (2, 3)
    case .closeLeftovers, .blocked: return nil
    }
  }
}
