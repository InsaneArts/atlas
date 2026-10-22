/// One screenshot in the Screenshots pane's grid: its thumbnail once read, its time, and whether it is selected.
package struct ViewerShotTile: Equatable, Sendable, Identifiable {
  /// The screenshot's file name, which identifies it.
  package var id: String
  /// When it was taken, such as "10:31:44".
  package var time: String
  /// Its thumbnail; nil until it is read.
  package var thumbnail: DesktopPicture?
  /// Whether it is the selected one, which Space opens.
  package var isSelected: Bool
  /// What VoiceOver reads for it.
  package var label: String

  /// Creates a tile.
  ///
  /// - Parameters:
  ///   - id: The screenshot's file name.
  ///   - time: When it was taken.
  ///   - thumbnail: Its thumbnail.
  ///   - isSelected: Whether it is selected.
  ///   - label: What VoiceOver reads for it.
  package init(id: String, time: String, thumbnail: DesktopPicture? = nil, isSelected: Bool = false, label: String) {
    self.id = id
    self.time = time
    self.thumbnail = thumbnail
    self.isSelected = isSelected
    self.label = label
  }
}
