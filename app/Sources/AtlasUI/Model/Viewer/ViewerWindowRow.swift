/// One of an app's windows in the Apps pane: its title and size, and Show on screen.
package struct ViewerWindowRow: Equatable, Sendable, Identifiable {
  /// The window's number in the helper's session.
  package var id: Int
  /// Its title, or what stands for none.
  package var title: String
  /// Its size in points, such as "520 × 860".
  package var size: String
  /// What VoiceOver calls Show on screen for this window.
  package var showLabel: String

  /// Creates a window's row.
  ///
  /// - Parameters:
  ///   - id: The window's number.
  ///   - title: Its title.
  ///   - size: Its size in points.
  ///   - showLabel: What VoiceOver calls Show on screen for it.
  package init(id: Int, title: String, size: String, showLabel: String) {
    self.id = id
    self.title = title
    self.size = size
    self.showLabel = showLabel
  }
}
