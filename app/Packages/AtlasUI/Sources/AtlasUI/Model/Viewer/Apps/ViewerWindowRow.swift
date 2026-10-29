/// One of an app's windows on its card: its title, and Show on Screen.
public struct ViewerWindowRow: Equatable, Sendable, Identifiable {
  /// The window's number in the helper's session.
  public var id: Int
  /// Its title, or what stands for none.
  public var title: String
  /// Its size in points, such as "520 × 860", which VoiceOver says.
  public var size: String
  /// What VoiceOver calls Show on screen for this window.
  public var showLabel: String
  /// Whether it is the front window of the app in front, which its row marks.
  public var isFront: Bool

  /// Creates a window's row.
  ///
  /// - Parameters:
  ///   - id: The window's number.
  ///   - title: Its title.
  ///   - size: Its size in points.
  ///   - showLabel: What VoiceOver calls Show on screen for it.
  ///   - isFront: Whether it is the front window of the app in front.
  public init(id: Int, title: String, size: String, showLabel: String, isFront: Bool = false) {
    self.id = id
    self.title = title
    self.size = size
    self.showLabel = showLabel
    self.isFront = isFront
  }
}
