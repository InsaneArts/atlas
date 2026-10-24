/// An explicit request to reveal a location without activating its control.
public struct SettingsReveal: Equatable, Sendable {
  /// The row or heading to reveal.
  public let target: SettingsSearchTarget
  /// Distinguishes repeated requests for the same row.
  public let request: Int

  /// Creates a reveal request.
  ///
  /// - Parameters:
  ///   - target: The row or heading.
  ///   - request: The request counter.
  public init(target: SettingsSearchTarget, request: Int) {
    self.target = target
    self.request = request
  }
}
