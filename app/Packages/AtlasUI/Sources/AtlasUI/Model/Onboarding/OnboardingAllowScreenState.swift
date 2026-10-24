/// In the helper: whether to let the account that asked use this one.
public struct OnboardingAllowScreenState: Equatable, Sendable {
  /// The asking account's first letter, on its disc.
  public var initial: String
  /// The label over the title, naming this account.
  public var label: String
  /// The headline, naming the asking account.
  public var title: String
  /// What allowing means, naming the asking account.
  public var lede: String

  /// Creates the helper's allow screen state.
  ///
  /// - Parameters:
  ///   - initial: The asking account's first letter, on its disc.
  ///   - label: The label over the title, naming this account.
  ///   - title: The headline, naming the asking account.
  ///   - lede: What allowing means, naming the asking account.
  public init(initial: String, label: String, title: String, lede: String) {
    self.initial = initial
    self.label = label
    self.title = title
    self.lede = lede
  }
}
