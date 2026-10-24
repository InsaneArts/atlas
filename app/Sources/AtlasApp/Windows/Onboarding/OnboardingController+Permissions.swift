import AtlasCommands
import AtlasOnboarding

/// The helper's own screens: Allow, each permission with the panel beside System Settings, and the switch back.
extension OnboardingController {
  /// Allows exactly the account shown: a request changed since is refused.
  func allow() async {
    let asker = snapshot.facts.asker
    await act(.allow(uid: asker.uid, name: asker.name), OnboardingWords.settingUpHere)
  }

  /// Opens the permission's list in System Settings, with the permission panel beside it.
  ///
  /// - Parameter permission: The permission to allow.
  func openSettings(_ permission: Permission) {
    if let url = permission.settingsURL { dependencies.openURL(url) }
    snapshot.guiding = permission
  }

  /// Once `permission`'s tick has shown, allows the next permission still off, opening its list in System Settings,
  /// or, with both on, closes the panel for the helper's done screen.
  ///
  /// - Parameter permission: The permission just turned on.
  func moveOn(from permission: Permission) async {
    try? await dependencies.clock.sleep(for: Self.tickShown)
    // Back during the tick: a panel opened again since is not this one's to move on.
    guard !Task.isCancelled, !closed, snapshot.guiding == permission else { return }
    if let next = Permission.toGuide(after: permission, asker: snapshot.facts.asker), next != permission {
      openSettings(next)
    } else {
      snapshot.guiding = nil
    }
  }

  /// Back on the permission panel, or System Settings gone: the permissions screen again. The tick under way stops,
  /// and the panel's requests still waiting are dropped.
  func closePanel() {
    stop(where: \.endsWithPanel)
    snapshot.guiding = nil
  }

  /// Locks this account for the switch back, to the account that asked, or with none named when no one asked; the
  /// window's work here is then done.
  func switchBack() async {
    let name = snapshot.facts.asker.name
    let action: Action = name.isEmpty ? .lockScreen : .switchTo(name)
    if let outcome = await act(action, Words.locking), outcome.ok { closeRequested = true }
  }
}
