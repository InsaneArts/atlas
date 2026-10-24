import AtlasOnboarding
import AtlasUI

extension OnboardingPresenter {
  /// The allow screen, naming the account that asked and this one, with the asking account's picture where it has
  /// one.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The allow screen.
  static func allow(_ snapshot: OnboardingSnapshot) -> OnboardingAllowScreenState {
    let facts = snapshot.facts
    let asker = facts.asker.title
    return OnboardingAllowScreenState(
      initial: asker.initial,
      picture: snapshot.askerPicture,
      label: OnboardingWords.atlasIn(facts.myName),
      title: OnboardingWords.letUse(asker),
      lede: OnboardingWords.allowLede(asker)
    )
  }

  /// The two permissions, each saying where it stands; the next one to allow is the footer's button.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The permissions screen.
  static func permissions(_ snapshot: OnboardingSnapshot) -> OnboardingPermissionsScreenState {
    OnboardingPermissionsScreenState(
      accessibility: item(.accessibility, snapshot: snapshot),
      screenRecording: item(.screenRecording, snapshot: snapshot)
    )
  }

  /// The permission whose turn it is, the next one to allow; nil while the permissions cannot be read, or once both
  /// are on.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The permission, or nil.
  static func turn(_ snapshot: OnboardingSnapshot) -> Permission? {
    let asker = snapshot.facts.asker
    guard asker.permissionsKnown else { return nil }
    return Permission.toGuide(after: nil, asker: asker)
  }

  /// One permission as its row shows it, named as this Mac's System Settings names it: Allowed once on; Checking…
  /// while it cannot be read, never off; Turned off when this window saw it on before; nothing while its turn has
  /// come, as the footer offers it; else Next.
  ///
  /// - Parameters:
  ///   - permission: The permission.
  ///   - snapshot: What the controller holds.
  /// - Returns: The permission's item.
  private static func item(_ permission: Permission, snapshot: OnboardingSnapshot) -> OnboardingPermissionItem {
    let asker = snapshot.facts.asker
    let on = permission.granted(to: asker)
    let isTurn = turn(snapshot) == permission
    let standing: OnboardingPermissionStanding =
      if on {
        .allowed(StatusWord.allowed.text)
      } else if !asker.permissionsKnown {
        .checking(StatusWord.checking.text)
      } else if snapshot.allowedBefore.contains(permission) {
        .turnedOff(StatusWord.turnedOff.text)
      } else if isTurn {
        .yourTurn
      } else {
        .next(StatusWord.next.text)
      }
    return OnboardingPermissionItem(
      id: permission.id,
      title: permission.title(on: snapshot.macOS),
      isOn: on,
      standing: standing,
      isCurrent: isTurn,
      isKnown: asker.permissionsKnown
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

  /// The panel beside System Settings while `permission` is allowed: which of how many, where to drag Atlas, the way
  /// for Atlas listed already, both permissions with only this one lit, and its status: Waiting for you… while the
  /// person acts, Checking… while the permissions cannot be read, as Atlas's job restarts, then Allowed. The label,
  /// the steps and the status all follow the permission guided, so they change together as the panel moves on.
  ///
  /// - Parameters:
  ///   - permission: The permission allowed now.
  ///   - snapshot: What the controller holds.
  /// - Returns: The panel.
  static func panel(_ permission: Permission, snapshot: OnboardingSnapshot) -> OnboardingPermissionPanelState {
    let asker = snapshot.facts.asker
    let title = permission.title(on: snapshot.macOS)
    let status: OnboardingPermissionStanding =
      if permission.granted(to: asker) {
        .allowed(StatusWord.allowed.text)
      } else if !asker.permissionsKnown {
        .checking(StatusWord.checking.text)
      } else {
        .waiting(StatusWord.waitingForYou.text)
      }
    let guided = { (other: Permission) in
      var item = item(other, snapshot: snapshot)
      item.isCurrent = other == permission && !item.isOn
      return item
    }
    return OnboardingPermissionPanelState(
      label: OnboardingWords.panelLabel(permission.number, of: Permission.allCases.count, title),
      instruction: OnboardingWords.dragInto(permission),
      alreadyListed: OnboardingWords.alreadyListed(permission),
      accessibility: guided(.accessibility),
      screenRecording: guided(.screenRecording),
      status: status
    )
  }
}
