import AtlasOnboarding
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
      label: OnboardingWords.atlasIn(facts.myName),
      title: OnboardingWords.letUse(asker),
      lede: OnboardingWords.allowLede(asker)
    )
  }

  /// The two permissions, the next one still off with its Allow… button; none while they are unknown.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The permissions screen.
  static func permissions(_ snapshot: OnboardingSnapshot) -> OnboardingPermissionsScreenState {
    OnboardingPermissionsScreenState(
      accessibility: item(.accessibility, snapshot: snapshot),
      screenRecording: item(.screenRecording, snapshot: snapshot)
    )
  }

  /// One permission as its row shows it, named as this Mac's System Settings names it; while it cannot be read, it is
  /// being checked, never off.
  ///
  /// - Parameters:
  ///   - permission: The permission.
  ///   - snapshot: What the controller holds.
  /// - Returns: The permission's item.
  private static func item(_ permission: Permission, snapshot: OnboardingSnapshot) -> OnboardingPermissionItem {
    let asker = snapshot.facts.asker
    let on = permission.granted(to: asker)
    return OnboardingPermissionItem(
      id: permission.id,
      title: permission.title(on: snapshot.macOS),
      isOn: on,
      status: status(on: on, known: asker.permissionsKnown),
      allowTitle: OnboardingWords.allowButton,
      canAllow: asker.permissionsKnown && Permission.toGuide(after: nil, asker: asker) == permission,
      isKnown: asker.permissionsKnown
    )
  }

  /// A permission's status in words.
  ///
  /// - Parameters:
  ///   - on: Whether it is on.
  ///   - known: Whether the permissions were read.
  /// - Returns: Allowed, Off, or Checking… while unknown.
  private static func status(on: Bool, known: Bool) -> String {
    switch (on, known) {
    case (true, _): OnboardingWords.permissionOn
    case (false, true): OnboardingWords.permissionOff
    case (false, false): OnboardingWords.permissionUnknown
    }
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

  /// The panel beside System Settings while `permission` is allowed: which of how many, where to drag Atlas, the way
  /// for Atlas listed already, both permissions, and its pill ticked once it is on, or reading Checking… while the
  /// permissions cannot be read, as Atlas's job restarts.
  ///
  /// - Parameters:
  ///   - permission: The permission allowed now.
  ///   - snapshot: What the controller holds.
  /// - Returns: The panel.
  static func panel(_ permission: Permission, snapshot: OnboardingSnapshot) -> OnboardingPermissionPanelState {
    let asker = snapshot.facts.asker
    let on = permission.granted(to: asker)
    let title = permission.title(on: snapshot.macOS)
    let waiting = asker.permissionsKnown ? OnboardingWords.waitingForSwitch : OnboardingWords.permissionUnknown
    return OnboardingPermissionPanelState(
      label: OnboardingWords.panelLabel(permission.number, of: Permission.allCases.count, title),
      instruction: OnboardingWords.dragInto(permission),
      alreadyListed: OnboardingWords.alreadyListed(permission),
      accessibility: item(.accessibility, snapshot: snapshot),
      screenRecording: item(.screenRecording, snapshot: snapshot),
      pill: on ? OnboardingWords.permissionOn : waiting,
      isOn: on
    )
  }
}
