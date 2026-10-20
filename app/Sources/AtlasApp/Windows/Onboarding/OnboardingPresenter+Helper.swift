import AtlasKit
import AtlasUI

extension OnboardingPresenter {
  /// The allow screen, naming the account that asked and this one.
  ///
  /// - Parameter facts: What `atlas onboarding status` reported.
  /// - Returns: The allow screen.
  static func allow(_ facts: OnboardingFacts) -> OnboardingAllowScreenState {
    let asker = facts.asker.title
    return OnboardingAllowScreenState(
      initial: asker.initial,
      eyebrow: OnboardingWords.atlasIn(facts.myName),
      title: OnboardingWords.letUse(asker),
      lede: OnboardingWords.allowLede(asker)
    )
  }

  /// The two permissions, and the first one still off, Accessibility before Screen Recording.
  ///
  /// - Parameter asker: This account's state, in the helper.
  /// - Returns: The permissions screen.
  static func permissions(_ asker: OnboardingFacts.Asker) -> OnboardingPermissionsScreenState {
    let next = Permission.allCases
      .first { !$0.granted(to: asker) }
    return OnboardingPermissionsScreenState(
      accessibility: item(.accessibility, asker: asker),
      screenRecording: item(.screenRecording, asker: asker),
      next: next.map { item($0, asker: asker) }
    )
  }

  /// One permission as its row and button show it.
  ///
  /// - Parameters:
  ///   - permission: The permission.
  ///   - asker: This account's state, in the helper.
  /// - Returns: The permission's item.
  private static func item(_ permission: Permission, asker: OnboardingFacts.Asker) -> OnboardingPermissionItem {
    let on = permission.granted(to: asker)
    return OnboardingPermissionItem(
      id: permission.id,
      title: permission.title,
      isOn: on,
      status: on ? OnboardingWords.permissionOn : OnboardingWords.permissionOff,
      allowTitle: OnboardingWords.allow(permission.title)
    )
  }

  /// The helper's done screen, naming the account to switch back to, or the person's own when none asked.
  ///
  /// - Parameter asker: The account that asked, in the helper.
  /// - Returns: The helper's done screen.
  static func helperDone(_ asker: OnboardingFacts.Asker) -> OnboardingHelperDoneScreenState {
    OnboardingHelperDoneScreenState(
      tail: asker.name.isEmpty ? OnboardingWords.switchBackToOwn : OnboardingWords.switchBack(to: asker.title)
    )
  }

  /// The guide beside System Settings: which permission of how many, where its switch is, and its steps ticked once
  /// the switch is on.
  ///
  /// - Parameters:
  ///   - permission: The permission whose switch is turned on.
  ///   - asker: This account's state, in the helper.
  /// - Returns: The guide.
  static func guide(_ permission: Permission, asker: OnboardingFacts.Asker) -> OnboardingGuideState {
    let on = permission.granted(to: asker)
    let title = permission.title
    return OnboardingGuideState(
      eyebrow: OnboardingWords.guideEyebrow(permission.number, of: Permission.allCases.count, title),
      settingsPath: OnboardingWords.settingsPath(title),
      turnOn: on ? .done : .current,
      comeBack: on ? .done : .waiting,
      pill: on ? OnboardingWords.isOn(title) : OnboardingWords.waiting(for: title),
      isOn: on
    )
  }
}
