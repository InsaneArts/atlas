/// One of an app's windows in the Apps pane: its title and size, and Show on screen.
public struct ViewerWindowRow: Equatable, Sendable, Identifiable {
  /// The window's number in the helper's session.
  public var id: Int
  /// Its title, or what stands for none.
  public var title: String
  /// Its size in points, such as "520 × 860".
  public var size: String
  /// What VoiceOver calls Show on screen for this window.
  public var showLabel: String

  /// Creates a window's row.
  ///
  /// - Parameters:
  ///   - id: The window's number.
  ///   - title: Its title.
  ///   - size: Its size in points.
  ///   - showLabel: What VoiceOver calls Show on screen for it.
  public init(id: Int, title: String, size: String, showLabel: String) {
    self.id = id
    self.title = title
    self.size = size
    self.showLabel = showLabel
  }
}
