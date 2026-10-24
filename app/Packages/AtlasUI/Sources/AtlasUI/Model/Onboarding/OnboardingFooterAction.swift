/// One button in the setup window's footer.
public struct OnboardingFooterAction: Equatable, Sendable {
  /// Which button it is, which its fixed title follows.
  public enum Title: Equatable, Sendable {
    /// Back.
    case back
    /// Choose Another Account, in Back's place: the helper asked never answered.
    case chooseAnother
    /// Continue.
    case `continue`
    /// Create Account.
    case createAccount
    /// Done.
    case done
  }

  /// Which button it is, which its fixed title follows.
  public var title: Title
  /// What pressing it sends.
  public var event: OnboardingEvent
  /// Whether it can be pressed now.
  public var isEnabled: Bool
  /// Whether Return presses it.
  public var isDefault: Bool

  /// Creates a footer button.
  ///
  /// - Parameters:
  ///   - title: Which button it is, which its fixed title follows.
  ///   - event: What pressing it sends.
  ///   - isEnabled: Whether it can be pressed now.
  ///   - isDefault: Whether Return presses it.
  public init(title: Title, event: OnboardingEvent, isEnabled: Bool, isDefault: Bool) {
    self.title = title
    self.event = event
    self.isEnabled = isEnabled
    self.isDefault = isDefault
  }
}
