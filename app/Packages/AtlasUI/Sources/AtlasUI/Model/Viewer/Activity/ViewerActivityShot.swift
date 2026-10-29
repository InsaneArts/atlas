import CoreGraphics

/// The screenshot a step of the Activity pane took: its file name, which Show large opens, its shape, and its
/// thumbnail once read. The step's own label names it for VoiceOver.
public struct ViewerActivityShot: Equatable, Sendable {
  /// 16:10: a screenshot's shape when the service does not say it, as the helper's displays mostly are.
  public static let defaultAspect = ShotShape.standard

  /// The screenshot's file name.
  public var name: String
  /// Its width over its height as the screenshots list says it.
  public var aspect: CGFloat
  /// Its thumbnail; nil until it is read.
  public var thumbnail: DesktopPicture?

  /// Creates a step's screenshot.
  ///
  /// - Parameters:
  ///   - name: The screenshot's file name.
  ///   - aspect: Its width over its height; 16:10 by default.
  ///   - thumbnail: Its thumbnail; nil until it is read.
  public init(name: String, aspect: CGFloat = defaultAspect, thumbnail: DesktopPicture? = nil) {
    self.name = name
    self.aspect = aspect
    self.thumbnail = thumbnail
  }

  /// The shape its picture is drawn at, so the whole screenshot shows, never cut (``ShotShape``).
  public var shape: CGFloat {
    ShotShape.of(thumbnail, listed: aspect)
  }
}
