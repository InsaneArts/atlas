/// An explicit request to reveal a location without activating its control.
public struct SettingsReveal: Equatable, Sendable {
  /// The row or heading to reveal.
  public let target: SettingsSearchTarget
  /// Distinguishes repeated requests for the same row.
  public let request: Int
  /// Whether this request also moves the page's retained scroll position.
  public let scrolls: Bool

  /// Creates a reveal request.
  ///
  /// - Parameters:
  ///   - target: The row or heading.
  ///   - request: The request counter.
  ///   - scrolls: Moves the scroll position for a fresh navigation or search result.
  public init(target: SettingsSearchTarget, request: Int, scrolls: Bool = true) {
    self.target = target
    self.request = request
    self.scrolls = scrolls
  }
}
