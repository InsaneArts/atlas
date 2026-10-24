import AtlasOnboarding

/// One thing the setup window does in its queue, which ``OnboardingController/run()`` performs in order, one at a time
/// (`OnboardingController+Queue`): the person's actions, the check and the agents reading a step starts by itself, and
/// item 3's permission panel. A reading, ``refresh``, never waits in the queue: the reading loop takes it at once.
///
/// A request already waiting or under way is not queued again.
nonisolated enum OnboardingRequest: Hashable, Sendable {
  /// Continue on the account choice: the form for a new account, else the picked account set up as the helper.
  case useHelper
  /// Create Account on the form.
  case createAccount
  /// Lock and Switch, to log in to the helper.
  case lockAndSwitch
  /// Choose Another Account, while the helper asked never answered.
  case chooseAnother
  /// Continue on the agents: connect the chosen ones.
  case connectAgents
  /// Done: Open at Login set, Atlas opened, the window closed.
  case complete
  /// Allow, in the helper: the account that asked.
  case allow
  /// Switch Back, in the helper.
  case switchBack
  /// Read the facts again soon: Check Again, or Continue once the check passed.
  case refresh
  /// The end-to-end check, which the finishing step starts by itself.
  case check
  /// Which agents are installed and connected, which the agents step asks by itself.
  case readAgents
  /// Allow a permission: its page in System Settings opens, with the permission panel beside it.
  ///
  /// - Parameter permission: The permission.
  case openSettings(Permission)
  /// A permission just turned on: once its tick has shown, the permission is read again and the next one still off
  /// opens, or the panel steps aside.
  ///
  /// - Parameter permission: The permission just turned on.
  case moveOn(from: Permission)

  /// Whether Back stops it, waiting or under way: the lock's wait, which leaves nothing half done. A command that sets
  /// something up, an account, a helper, an install or the check, always runs to its end.
  var endsOnBack: Bool {
    switch self {
    case .lockAndSwitch, .switchBack: true
    case .useHelper, .createAccount, .chooseAnother, .connectAgents, .complete, .allow, .refresh, .check, .readAgents,
      .openSettings, .moveOn:
      false
    }
  }

  /// Whether closing the permission panel stops it, waiting or under way: the panel's own requests.
  var endsWithPanel: Bool {
    switch self {
    case .openSettings, .moveOn: true
    case .useHelper, .createAccount, .lockAndSwitch, .chooseAnother, .connectAgents, .complete, .allow, .switchBack,
      .refresh, .check, .readAgents:
      false
    }
  }
}
