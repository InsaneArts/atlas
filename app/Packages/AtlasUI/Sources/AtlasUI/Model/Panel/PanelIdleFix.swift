/// What the note that the person's apps think they are away tells them to do: quit the app that keeps secure keyboard
/// entry on.
public struct PanelIdleFix: Equatable, Sendable {
  /// The app to quit, by the name it shows; nil when Atlas could not tell which app it is.
  public var app: String?

  /// Creates the fix.
  ///
  /// - Parameter app: The app to quit; nil when Atlas could not tell which.
  public init(app: String?) {
    self.app = app
  }
}
