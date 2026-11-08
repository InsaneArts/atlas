import CoreGraphics

/// One picture of the Activity outline, in its one frame: a screenshot a step took, by its file name, its thumbnail
/// once read, whether it shows the whole screen, and whether the picture sheet shows it now.
public struct ViewerPictureTile: Equatable, Sendable, Identifiable {
  /// The screenshot's file name, which identifies it and which the sheet opens.
  public var id: String
  /// Its thumbnail; nil until it is read.
  public var thumbnail: DesktopPicture?
  /// Whether it shows the whole screen, which fills the frame, rather than a window, which stands in it.
  public var fillsFrame: Bool
  /// Whether it is the picture the sheet shows now.
  public var isCurrent: Bool
  /// Whether it opened the sheet, whose picture the outline does not show now, so it takes focus back as the sheet
  /// closes.
  public var returnsFocus: Bool
  /// What VoiceOver reads for it, such as "Screenshot, 10:31".
  public var label: String

  /// Creates a picture.
  ///
  /// - Parameters:
  ///   - id: The screenshot's file name.
  ///   - thumbnail: Its thumbnail; none until read.
  ///   - fillsFrame: Whether it shows the whole screen; it shows a window by default.
  ///   - isCurrent: Whether the sheet shows it now; it does not by default.
  ///   - returnsFocus: Whether it takes focus back as the sheet closes; it does not by default.
  ///   - label: What VoiceOver reads for it.
  public init(
    id: String,
    thumbnail: DesktopPicture? = nil,
    fillsFrame: Bool = false,
    isCurrent: Bool = false,
    returnsFocus: Bool = false,
    label: String
  ) {
    self.id = id
    self.thumbnail = thumbnail
    self.fillsFrame = fillsFrame
    self.isCurrent = isCurrent
    self.returnsFocus = returnsFocus
    self.label = label
  }
}
