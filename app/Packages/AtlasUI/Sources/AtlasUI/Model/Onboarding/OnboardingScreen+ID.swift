extension OnboardingScreen {
  /// Which screen this is, whatever it shows: a new id moves the window to a new screen with its transition, while
  /// a change on the same screen redraws it in place.
  public enum ID: Hashable, Sendable {
    /// The welcome.
    case welcome
    /// The account choice.
    case choose
    /// The account form.
    case create
    /// The switch to the helper.
    case switchToHelper
    /// The finishing check.
    case finishing
    /// The agents.
    case agents
    /// Done.
    case done
    /// The helper's allow screen.
    case allow
    /// The helper's permissions.
    case permissions
    /// The helper's done screen.
    case helperDone
    /// The close screen, before and once it closed: the same screen, redrawn in place.
    case closeLeftovers
    /// A blocked screen, one per reason.
    ///
    /// - Parameter reason: What to fix.
    case blocked(String)
  }

  /// Which screen this is.
  public var id: ID {
    switch self {
    case .welcome: .welcome
    case .choose: .choose
    case .create: .create
    case .switchToHelper: .switchToHelper
    case .finishing: .finishing
    case .agents: .agents
    case .done: .done
    case .allow: .allow
    case .permissions: .permissions
    case .helperDone: .helperDone
    case .closeLeftovers: .closeLeftovers
    case .blocked(let reason): .blocked(reason)
    }
  }
}
