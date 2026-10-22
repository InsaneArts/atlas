/// An app open in the helper, as the Apps pane lists it: its icon and name, whether it is in front, and its windows
/// front to back.
package struct ViewerAppRow: Equatable, Sendable, Identifiable {
  /// The app's process id in the helper.
  package var id: Int
  /// Its name.
  package var name: String
  /// Its icon; nil draws a plain tile.
  package var icon: IconImage?
  /// Whether it is the helper's frontmost app.
  package var isFront: Bool
  /// Its windows, front to back; none shows "No windows".
  package var windows: [ViewerWindowRow]

  /// Creates an app's row.
  ///
  /// - Parameters:
  ///   - id: The app's process id.
  ///   - name: Its name.
  ///   - icon: Its icon.
  ///   - isFront: Whether it is in front.
  ///   - windows: Its windows, front to back.
  package init(id: Int, name: String, icon: IconImage? = nil, isFront: Bool = false, windows: [ViewerWindowRow]) {
    self.id = id
    self.name = name
    self.icon = icon
    self.isFront = isFront
    self.windows = windows
  }
}
