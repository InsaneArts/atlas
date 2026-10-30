/// The footer's button for agents: Stop while one acts, Resume in its place once the person stopped them.
public enum ViewerAgentButton: Equatable, Sendable {
  /// Stop. It never waits for anything.
  case stop
  /// Resume.
  ///
  /// - Parameter isEnabled: Whether it can be pressed: nothing else is asked of the service.
  case resume(isEnabled: Bool)

  /// The full action name shared with the menu, even when the button shows a short word.
  public var accessibilityLabel: String {
    switch self {
    case .stop: HelperActionNames.stopAgent
    case .resume: HelperActionNames.resumeAgent
    }
  }
}
