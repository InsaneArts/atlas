/// A deleted account's login is still open: what the close screen says before Atlas closes it, and once it has.
public struct OnboardingCloseScreenState: Equatable, Sendable {
  /// The headline.
  public var title: String
  /// What happened and what closing it does; nil once it is closed.
  public var lede: String?
  /// Whether Atlas has closed it, so the screen shows the success hero and setup can go on.
  public var isClosed: Bool

  /// Creates the close screen's state.
  ///
  /// - Parameters:
  ///   - title: The headline.
  ///   - lede: What happened and what closing it does; nil once it is closed.
  ///   - isClosed: Whether Atlas has closed it.
  public init(title: String, lede: String?, isClosed: Bool) {
    self.title = title
    self.lede = lede
    self.isClosed = isClosed
  }
}
