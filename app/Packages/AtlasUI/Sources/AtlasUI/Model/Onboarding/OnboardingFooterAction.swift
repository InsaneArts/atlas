/// One button in the setup window's footer.
public struct OnboardingFooterAction: Equatable, Sendable {
  /// Which button it is, which its fixed title follows.
  public enum Title: Equatable, Sendable {
    /// Back.
    case back
    /// Choose Another Account, in Back's place: the helper asked never answered.
    case chooseAnother
    /// Get Started, on the welcome.
    case getStarted
    /// Continue.
    case `continue`
    /// Create Account.
    case createAccount
    /// Lock and Switch, to log in to the helper.
    case lockAndSwitch
    /// Try Again, after the first reading or the check failed.
    case tryAgain
    /// Check Again, when something only the person can fix comes first.
    case checkAgain
    /// Done.
    case done
    /// Allow, in the helper.
    case allow
    /// Switch Back, in the helper.
    case switchBack

    /// Whether the screen drew this button itself before every main action moved to the footer (v3 D3): the
    /// buttons the footer hands back to the screen while ``SwiftUICore/EnvironmentValues/mainActionInFooter`` is off.
    var isScreenAction: Bool {
      switch self {
      case .getStarted, .lockAndSwitch, .tryAgain, .checkAgain, .allow, .switchBack: true
      case .back, .chooseAnother, .continue, .createAccount, .done: false
      }
    }
  }

  /// Which button it is, which its fixed title follows.
  public var title: Title
  /// What pressing it sends.
  public var event: OnboardingEvent
  /// Whether it can be pressed now.
  public var isEnabled: Bool
  /// Whether Return presses it. Only the main action, on the right, ever takes Return.
  public var isDefault: Bool

  /// Creates a footer button.
  ///
  /// - Parameters:
  ///   - title: Which button it is, which its fixed title follows.
  ///   - event: What pressing it sends.
  ///   - isEnabled: Whether it can be pressed now.
  ///   - isDefault: Whether Return presses it; only the main action, on the right, ever takes Return.
  public init(title: Title, event: OnboardingEvent, isEnabled: Bool, isDefault: Bool) {
    self.title = title
    self.event = event
    self.isEnabled = isEnabled
    self.isDefault = isDefault
  }
}
