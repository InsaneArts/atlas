import CoreGraphics

/// One screenshot in the Screenshots pane's grid: its thumbnail once read, its shape, its time, and whether it is
/// selected.
public struct ViewerShotTile: Equatable, Sendable, Identifiable {
  /// The screenshot's file name, which identifies it.
  public var id: String
  /// When it was taken, such as "10:31:44".
  public var time: String
  /// Its thumbnail; nil until it is read.
  public var thumbnail: DesktopPicture?
  /// Its width over its height as the screenshots list says it; nil when the list does not.
  public var aspect: CGFloat?
  /// Whether it is the selected one, which Space opens.
  public var isSelected: Bool
  /// What VoiceOver reads for it.
  public var label: String

  /// Creates a tile.
  ///
  /// - Parameters:
  ///   - id: The screenshot's file name.
  ///   - time: When it was taken.
  ///   - thumbnail: Its thumbnail.
  ///   - aspect: Its width over its height as the list says it.
  ///   - isSelected: Whether it is selected.
  ///   - label: What VoiceOver reads for it.
  public init(
    id: String,
    time: String,
    thumbnail: DesktopPicture? = nil,
    aspect: CGFloat? = nil,
    isSelected: Bool = false,
    label: String
  ) {
    self.id = id
    self.time = time
    self.thumbnail = thumbnail
    self.aspect = aspect
    self.isSelected = isSelected
    self.label = label
  }

  /// The shape its picture is drawn at, so the whole screenshot shows, never cut (``ShotShape``).
  public var shape: CGFloat {
    ShotShape.of(thumbnail, listed: aspect)
  }
}
