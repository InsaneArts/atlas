/// One button in the setup window's footer.
package struct OnboardingFooterAction: Equatable, Sendable {
  /// Which button it is, which its fixed title follows.
  package enum Title: Equatable, Sendable {
    /// Back.
    case back
    /// Continue.
    case `continue`
    /// Create Account.
    case createAccount
    /// Done.
    case done
  }

  /// Which button it is, which its fixed title follows.
  package var title: Title
  /// What pressing it sends.
  package var event: OnboardingEvent
  /// Whether it can be pressed now.
  package var isEnabled: Bool
  /// Whether Return presses it.
  package var isDefault: Bool

  /// Creates a footer button.
  ///
  /// - Parameters:
  ///   - title: Which button it is, which its fixed title follows.
  ///   - event: What pressing it sends.
  ///   - isEnabled: Whether it can be pressed now.
  ///   - isDefault: Whether Return presses it.
  package init(title: Title, event: OnboardingEvent, isEnabled: Bool, isDefault: Bool) {
    self.title = title
    self.event = event
    self.isEnabled = isEnabled
    self.isDefault = isDefault
  }
}
