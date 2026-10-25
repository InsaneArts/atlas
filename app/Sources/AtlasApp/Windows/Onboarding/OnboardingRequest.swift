import AtlasCommands
import AtlasOnboarding

/// One thing the setup window does in its queue, which ``OnboardingController/run()`` performs in order, one at a time
/// (`OnboardingController+Queue`): the person's actions, the check and the agents reading a step starts by itself, and
/// the permission panel's own requests. Fact readings run separately in the reading loop.
///
/// A request already waiting or under way is not queued again (``repeats(_:)``).
nonisolated enum OnboardingRequest: Hashable, Sendable {
  /// Continue on the account choice: the form for a new account, else the picked account set up as the helper.
  case useHelper
  /// Create Account on the form.
  case createAccount
  /// Lock and Switch, to log in to the helper.
  case lockAndSwitch
  /// Choose Another Account, while the helper asked never answered.
  case chooseAnother
  /// Close It…: the logins deleted accounts left open.
  case closeLeftovers
  /// Continue on the agents: connect the chosen ones.
  ///
  /// - Parameter chosen: The agents switched on when Continue was pressed. Only these are connected, so a reading
  ///   that lands while Continue waits its turn, switching on the agents it finds, connects none the person did not
  ///   choose, and a skipped step stays skipped.
  case connectAgents(chosen: [AgentID])
  /// Done: Open at Login set, Atlas opened, the window closed.
  case complete
  /// Allow, in the helper: the account approved when the button was pressed.
  ///
  /// - Parameters:
  ///   - uid: The approved account's user ID.
  ///   - name: The approved account's short name.
  case allow(uid: Int, name: String)
  /// Switch Back, in the helper.
  case switchBack
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

  /// Whether it does what `other` does, so it is not queued while `other` waits or runs: the same request, or a second
  /// Continue on the agents or Allow, whatever each had chosen.
  ///
  /// - Parameter other: The request waiting or under way.
  /// - Returns: Whether it does.
  func repeats(_ other: OnboardingRequest) -> Bool {
    switch (self, other) {
    case (.connectAgents, .connectAgents), (.allow, .allow): true
    default: self == other
    }
  }

  /// Whether Back stops it, waiting or under way: the lock's wait, which leaves nothing half done. A command that sets
  /// something up, an account, a helper, an install, the check or closing a login, always runs to its end.
  var endsOnBack: Bool {
    switch self {
    case .lockAndSwitch, .switchBack: true
    case .useHelper, .createAccount, .chooseAnother, .closeLeftovers, .connectAgents, .complete, .allow,
      .check, .readAgents, .openSettings, .moveOn:
      false
    }
  }

  /// Whether closing the permission panel stops it, waiting or under way: the panel's own requests.
  var endsWithPanel: Bool {
    switch self {
    case .openSettings, .moveOn: true
    case .useHelper, .createAccount, .lockAndSwitch, .chooseAnother, .closeLeftovers, .connectAgents, .complete,
      .allow, .switchBack, .check, .readAgents:
      false
    }
  }
}
