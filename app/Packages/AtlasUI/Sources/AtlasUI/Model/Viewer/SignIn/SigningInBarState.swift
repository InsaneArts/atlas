/// The bar over the viewer's Screen while the person signs in, deletes a site's data or clears the browser, agents
/// paused: what to do and that agents can't see, and Cancel and Done.
public struct SigningInBarState: Equatable, Sendable {
  /// What the bar says, such as "Signing in to github.com. Agents are paused and can't see the screen."
  public var text: String
  /// Whether Done or Cancel runs now, so both wait.
  public var isWorking: Bool

  /// Creates the bar's state.
  ///
  /// - Parameters:
  ///   - text: What it says.
  ///   - isWorking: Whether Done or Cancel runs now; not by default.
  public init(text: String, isWorking: Bool = false) {
    self.text = text
    self.isWorking = isWorking
  }
}
